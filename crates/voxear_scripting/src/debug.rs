//! Debug draw from C# (`Voxear.Debug`), into the process's list in
//! `voxear_core::debug_draw`. Vectors, rotations and colours come as
//! pointers to C#'s own structs, read unaligned.

use voxear_core::{
    debug_draw,
    math::{Quat, Vec3},
};

use crate::interop::utf8;

/// `flags` bits from `Voxear.Debug`.
const ON_TOP: i32 = 1;
const WITH_HEAD: i32 = 2;

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
    let on_top = flags & ON_TOP != 0;
    match flags & WITH_HEAD != 0 {
        true => debug_draw::arrow(from, to, color, duration, on_top),
        false => debug_draw::line(from, to, color, duration, on_top),
    }
}

pub(crate) unsafe extern "C" fn debug_box(
    center: *const [f32; 3],
    half_size: *const [f32; 3],
    rotation: *const [f32; 4],
    color_at: *const [f32; 4],
    duration: f32,
    on_top: i32,
) {
    let rotation = Quat::from_array(unsafe { rotation.read_unaligned() });
    let (center, half_size, color) = unsafe { (vec3(center), vec3(half_size), color(color_at)) };
    debug_draw::wire_box(center, half_size, rotation, color, duration, on_top != 0);
}

pub(crate) unsafe extern "C" fn debug_sphere(
    center: *const [f32; 3],
    radius: f32,
    color_at: *const [f32; 4],
    duration: f32,
    on_top: i32,
) {
    let (center, color) = unsafe { (vec3(center), color(color_at)) };
    debug_draw::sphere(center, radius, color, duration, on_top != 0);
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
