//! The game's window and the mouse, for `Screen` and `Cursor` in C#: the
//! window's size as whatever shows the game has it, and what scripts ask of
//! the window and the mouse, through the [`ScriptServices`].
//!
//! [`ScriptServices`]: crate::ScriptServices

use crate::services::with_services;

/// How the mouse is held, as `Cursor.Lock` says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorLock {
    /// Free to leave the window.
    #[default]
    None,
    /// Held in place and hidden, so its moves turn a camera without limit.
    Locked,
    /// Kept inside the window.
    Confined,
}

impl CursorLock {
    fn from_code(code: i32) -> Self {
        match code {
            1 => Self::Locked,
            2 => Self::Confined,
            _ => Self::None,
        }
    }

    fn code(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Locked => 1,
            Self::Confined => 2,
        }
    }
}

/// The window's size in pixels, and whether it fills the screen.
pub(crate) unsafe extern "C" fn screen_get(width: *mut u32, height: *mut u32) -> i32 {
    let (wide, high, fullscreen) = with_services(|services| services.screen()).unwrap_or((0, 0, false));
    if !width.is_null() && !height.is_null() {
        unsafe {
            *width = wide;
            *height = high;
        }
    }
    i32::from(fullscreen)
}

pub(crate) unsafe extern "C" fn screen_set(width: u32, height: u32, fullscreen: i32) {
    with_services(|services| services.set_resolution(width, height, fullscreen != 0));
}

/// Whether the mouse shows, and how it is held, written to `lock`.
pub(crate) unsafe extern "C" fn cursor_get(lock: *mut i32) -> i32 {
    let (visible, held) = with_services(|services| services.cursor()).unwrap_or((true, CursorLock::None));
    if !lock.is_null() {
        unsafe { *lock = held.code() };
    }
    i32::from(visible)
}

pub(crate) unsafe extern "C" fn cursor_set(visible: i32, lock: i32) {
    with_services(|services| services.set_cursor(visible != 0, CursorLock::from_code(lock)));
}

/// The display's HDR output, as `Screen.Hdr` and its kin read and set it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HdrInfo {
    /// What is asked for: 0 off, 1 HDR where the display offers it, 2 HDR10
    /// alone, 3 scRGB alone.
    pub mode: i32,
    /// What the window shows: 0 SDR, 1 HDR10, 2 scRGB.
    pub output: i32,
    /// The HDR outputs the display offers, a bit each: 1 HDR10, 2 scRGB.
    pub offered: i32,
    /// How bright white paper is in nits: the brightness of the SDR picture,
    /// and of the UI, on an HDR display.
    pub paper_white: f32,
    /// The display's brightest, in nits, which highlights roll off towards.
    pub peak: f32,
}

impl Default for HdrInfo {
    fn default() -> Self {
        Self { mode: 0, output: 0, offered: 0, paper_white: 200.0, peak: 1000.0 }
    }
}

/// `Screen.Hdr` and its kin: the HDR output as it is written to `out`, after
/// it is set as `set` asks when it is given. 0, or -1 with no game.
pub(crate) unsafe extern "C" fn screen_hdr(set: *const HdrInfo, out: *mut HdrInfo) -> i32 {
    let asked = (!set.is_null()).then(|| unsafe { set.read_unaligned() });
    let Some(now) = with_services(|services| services.hdr(asked)) else { return -1 };
    if !out.is_null() {
        unsafe { out.write_unaligned(now) };
    }
    0
}
