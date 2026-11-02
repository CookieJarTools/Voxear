//! Debug draw from C# (`Voxear.Debug`), into the process's list in
//! `voxear_core::debug_draw`. Vectors, rotations and colours come as
//! pointers to C#'s own structs, read unaligned.

use voxear_core::{
    debug_draw,
    math::{Quat, Vec3},
};

use crate::interop::utf8;

/// `flags` bits from `Voxear.Debug` and `Voxear.Draw`: on top of the scene,
/// with an arrowhead, part of the game (kept in release builds), drawn by a
/// behaviour's gizmos, and in bits 8 to 23 the width in sixteenths of a
/// pixel, 0 for the usual width.
const ON_TOP: i32 = 1;
const WITH_HEAD: i32 = 2;
const IN_GAME: i32 = 4;
const GIZMO: i32 = 8;
const WIDTH_SHIFT: i32 = 8;

/// How the lines are drawn, as `flags` say.
fn style(flags: i32) -> debug_draw::LineStyle {
    debug_draw::LineStyle {
        on_top: flags & ON_TOP != 0,
        width: ((flags >> WIDTH_SHIFT) & 0xffff) as f32 / 16.0,
        in_game: flags & IN_GAME != 0,
        gizmo: flags & GIZMO != 0,
    }
}

unsafe fn vec3(at: *const [f32; 3]) -> Vec3 {
    Vec3::from_array(unsafe { at.read_unaligned() })
}

unsafe fn color(at: *const [f32; 4]) -> [f32; 4] {
    unsafe { at.read_unaligned() }
}

pub(crate) unsafe extern "C" fn debug_line(
    from: *const [f32; 3],
    to: *const [f32; 3],
    color_at: *const [f32; 4],
    duration: f32,
    flags: i32,
) {
    let (from, to, color) = unsafe { (vec3(from), vec3(to), color(color_at)) };
    match flags & WITH_HEAD != 0 {
        true => debug_draw::arrow(from, to, color, duration, style(flags)),
        false => debug_draw::line(from, to, color, duration, style(flags)),
    }
}

pub(crate) unsafe extern "C" fn debug_box(
    center: *const [f32; 3],
    half_size: *const [f32; 3],
    rotation: *const [f32; 4],
    color_at: *const [f32; 4],
    duration: f32,
    flags: i32,
) {
    let rotation = Quat::from_array(unsafe { rotation.read_unaligned() });
    let (center, half_size, color) = unsafe { (vec3(center), vec3(half_size), color(color_at)) };
    debug_draw::wire_box(center, half_size, rotation, color, duration, style(flags));
}

pub(crate) unsafe extern "C" fn debug_sphere(
    center: *const [f32; 3],
    radius: f32,
    color_at: *const [f32; 4],
    duration: f32,
    flags: i32,
) {
    let (center, color) = unsafe { (vec3(center), color(color_at)) };
    debug_draw::sphere(center, radius, color, duration, style(flags));
}

pub(crate) unsafe extern "C" fn debug_text(
    at: *const [f32; 3],
    text: *const u8,
    len: i32,
    color_at: *const [f32; 4],
    duration: f32,
) {
    let (at, color, text) = unsafe { (vec3(at), color(color_at), utf8(text, len)) };
    debug_draw::text(at, &text, color, duration);
}
