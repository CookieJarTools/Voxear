//! The game's UI from C# (`Voxear.UiDocument` and `UiElement`), through the
//! services the game lends while scripts run: documents and elements are
//! numbers, and -1 means none.

use crate::{
    interop::utf8,
    services::{UiAnswer, UiAsk, with_services},
};

pub(crate) unsafe extern "C" fn ui_load(path: *const u8, len: i32) -> i32 {
    let path = unsafe { utf8(path, len) };
    match with_services(|services| services.ui_load(&path)) {
        Some(Ok(document)) => document as i32,
        Some(Err(error)) => {
            tracing::error!(target: "script", "UiDocument.Load(\"{path}\"): {error:#}");
            -1
        }
        None => {
            tracing::error!(target: "script", "UiDocument.Load(\"{path}\"): there is no UI while scripts are not running");
            -1
        }
    }
}

pub(crate) unsafe extern "C" fn ui_find(document: i32, name: *const u8, len: i32) -> i32 {
    let name = unsafe { utf8(name, len) };
    with_services(|services| services.ui_find(document as u32, &name)).flatten().map_or(-1, |element| element as i32)
}

pub(crate) unsafe extern "C" fn ui_close(document: i32) {
    with_services(|services| services.ui_close(document as u32));
}

/// An element's text, written when it fits; its length in bytes either way,
/// or -1 for none.
pub(crate) unsafe extern "C" fn ui_text(element: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let Some(UiAnswer::Text(text)) = ask(element, UiAsk::Text) else { return -1 };
    if !buffer.is_null() && text.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len()) };
    }
    text.len() as i32
}

pub(crate) unsafe extern "C" fn ui_set_text(element: i32, text: *const u8, len: i32) {
    let text = unsafe { utf8(text, len) };
    ask(element, UiAsk::SetText(&text));
}

/// Removes (0), adds (1), toggles (2) or looks for (3) a class; 1 when the
/// element has it after.
pub(crate) unsafe extern "C" fn ui_class(element: i32, class: *const u8, len: i32, change: i32) -> i32 {
    let class = unsafe { utf8(class, len) };
    i32::from(ask(element, UiAsk::Class(&class, change.clamp(0, 3) as u8)) == Some(UiAnswer::Yes(true)))
}

/// Whether it shows; with `set` 0 or 1, hides or shows it first.
pub(crate) unsafe extern "C" fn ui_visible(element: i32, set: i32) -> i32 {
    if set >= 0 {
        ask(element, UiAsk::SetVisible(set != 0));
    }
    i32::from(ask(element, UiAsk::Visible) == Some(UiAnswer::Yes(true)))
}

pub(crate) unsafe extern "C" fn ui_clicked(element: i32) -> i32 {
    i32::from(ask(element, UiAsk::Clicked) == Some(UiAnswer::Yes(true)))
}

/// A slider's or toggle's value; with `set` non-zero, sets it to `value` first.
pub(crate) unsafe extern "C" fn ui_value(element: i32, set: i32, value: f32) -> f32 {
    if set != 0 {
        ask(element, UiAsk::SetValue(value));
    }
    match ask(element, UiAsk::Value) {
        Some(UiAnswer::Number(value)) => value,
        _ => 0.0,
    }
}

pub(crate) unsafe extern "C" fn ui_style(element: i32, style: *const u8, len: i32) {
    let style = unsafe { utf8(style, len) };
    ask(element, UiAsk::SetStyle(&style));
}

fn ask(element: i32, asked: UiAsk<'_>) -> Option<UiAnswer> {
    if element < 0 {
        return None;
    }
    with_services(|services| services.ui_ask(element as u32, asked))
}
