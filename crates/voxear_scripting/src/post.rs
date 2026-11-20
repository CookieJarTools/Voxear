//! Post-processing and the display for C#: `Post` in Voxear.Core reads the
//! settings in effect and lays its own over the level's, as JSON, through
//! the one call set here. The engine answers, as only it knows the level's
//! settings (see `voxear_render::ScriptPost`).

use std::sync::OnceLock;

use voxear_ecs::World;

use crate::interop::{utf8, with_world};

/// What a call asks, as `Post` in Voxear.Core numbers them.
pub const POST_IN_EFFECT: i32 = 0;
pub const POST_OVERRIDES: i32 = 1;
pub const POST_SET: i32 = 2;
pub const POST_RESET: i32 = 3;
/// The sun's and lamps' shadows, through the same call: `Lighting.Shadows`
/// reads them as a frame draws them, sets its own over the level's, and
/// takes them off again.
pub const SHADOWS_IN_EFFECT: i32 = 4;
pub const SHADOWS_SET: i32 = 5;
pub const SHADOWS_RESET: i32 = 6;
/// The sky, through the same call: `Lighting.Sky` reads it as a frame
/// draws it, sets settings of its own over the level's, and takes them off.
pub const SKY_IN_EFFECT: i32 = 7;
pub const SKY_SET: i32 = 8;
pub const SKY_RESET: i32 = 9;
/// How the sun looks, through the same call: what scripts set over the
/// level's `[sun]`, set again whole, and taken off.
pub const SUN_LOOK: i32 = 10;
pub const SUN_LOOK_SET: i32 = 11;
pub const SUN_LOOK_RESET: i32 = 12;
/// The level's Seasons or Climate, through the same call: read whole, and
/// changed setting by setting (`Seasons` and `Climate`).
pub const LEVEL_COMPONENT: i32 = 13;
/// Project Settings, through the same call: a section read whole, and one
/// a running game may change changed setting by setting (`ProjectSettings`).
pub const PROJECT_SECTION: i32 = 14;
pub const PROJECT_CHANGE: i32 = 15;

/// What answers `Post` for scripts: what `what` asks, given `text`, with
/// the text of its answer.
pub type PostCall = fn(&mut World, i32, &str) -> anyhow::Result<String>;

static CALL: OnceLock<PostCall> = OnceLock::new();

/// Sets what answers `Post` in scripts; the first set is kept.
pub fn set_post_call(call: PostCall) {
    let _ = CALL.set(call);
}

/// `Post`: what `what` asks, given `text`; its answer written into `out`
/// when it fits, and its length. -1 where it was refused, which the
/// Console is told of, or while scripts are not running.
pub(crate) unsafe extern "C" fn post_call(what: i32, text: *const u8, len: i32, out: *mut u8, capacity: i32) -> i32 {
    let Some(call) = CALL.get() else { return -1 };
    let given = unsafe { utf8(text, len) };
    let answer = with_world(|world| Some(call(world, what, &given)));
    let bytes = match answer {
        Some(Ok(answer)) => answer.into_bytes(),
        Some(Err(error)) => {
            tracing::warn!(target: "script", "Post: {error:#}");
            return -1;
        }
        None => return -1,
    };
    if !out.is_null() && capacity >= bytes.len() as i32 {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
    }
    bytes.len() as i32
}
