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

/// An empty document, for elements made from code, or -1.
pub(crate) unsafe extern "C" fn ui_new() -> i32 {
    match with_services(|services| services.ui_new()) {
        Some(Ok(document)) => document as i32,
        Some(Err(error)) => {
            tracing::error!(target: "script", "UiDocument.Create(): {error:#}");
            -1
        }
        None => {
            tracing::error!(target: "script", "UiDocument.Create(): there is no UI while scripts are not running");
            -1
        }
    }
}

/// Makes an element of `kind` at the end of `parent`, and returns it, or
/// -1 when the parent is gone.
pub(crate) unsafe extern "C" fn ui_create(parent: i32, kind: i32, text: *const u8, len: i32) -> i32 {
    let text = unsafe { utf8(text, len) };
    if parent < 0 {
        return -1;
    }
    let created = with_services(|services| services.ui_create(parent as u32, kind, &text)).flatten();
    created.map_or(-1, |element| element as i32)
}

pub(crate) unsafe extern "C" fn ui_remove(element: i32) {
    if element >= 0 {
        with_services(|services| services.ui_remove(element as u32));
    }
}

/// Whether its value changed since the last frame.
pub(crate) unsafe extern "C" fn ui_changed(element: i32) -> i32 {
    i32::from(ask(element, UiAsk::Changed) == Some(UiAnswer::Yes(true)))
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

/// Gives a list view `count` items, each row `item_height` tall; for a tree
/// view, `parents` holds `count` pairs, each item's id and its parent's, 0
/// for one at the top, in the order they show.
pub(crate) unsafe extern "C" fn ui_items(element: i32, count: i32, parents: *const u64, item_height: f32) {
    let count = count.max(0) as usize;
    let tree = (!parents.is_null()).then(|| unsafe { std::slice::from_raw_parts(parents.cast::<[u64; 2]>(), count) });
    ask(element, UiAsk::Items { count, tree, item_height });
}

/// What a key map shows: see [`UiAsk::Keymap`].
pub(crate) unsafe extern "C" fn ui_keymap(
    element: i32,
    scheme: i32,
    colors: i32,
    hidden: i32,
    map: *const u8,
    len: i32,
) {
    let map = unsafe { utf8(map, len) };
    let (scheme, colors) = (scheme.clamp(0, 1) as u8, colors.clamp(0, 1) as u8);
    ask(element, UiAsk::Keymap { scheme, colors, hidden: hidden != 0, map: &map });
}

/// The path of the control clicked on a key map since the last frame,
/// written when it fits; its length either way, 0 for none.
pub(crate) unsafe extern "C" fn ui_keymap_clicked(element: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let Some(UiAnswer::Text(path)) = ask(element, UiAsk::KeymapClicked) else { return 0 };
    if !buffer.is_null() && path.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(path.as_ptr(), buffer, path.len()) };
    }
    path.len() as i32
}

/// One of the texts an element holds besides its own: its tooltip (0), a
/// text field's placeholder (1), a dropdown's options (2), one a line, or
/// the picture an image shows (3), by project path or GUID.
pub(crate) unsafe extern "C" fn ui_set_string(element: i32, which: i32, text: *const u8, len: i32) {
    let text = unsafe { utf8(text, len) };
    match which {
        0 => ask(element, UiAsk::SetTooltip(&text)),
        1 => ask(element, UiAsk::SetPlaceholder(&text)),
        2 => ask(element, UiAsk::SetOptions(&text)),
        3 => ask(element, UiAsk::SetPicture(&text)),
        _ => None,
    };
}

pub(crate) unsafe extern "C" fn ui_style(element: i32, style: *const u8, len: i32) {
    let style = unsafe { utf8(style, len) };
    ask(element, UiAsk::SetStyle(&style));
}

/// Sets one property of an element's inline style (`element.Style.Left`),
/// or with an empty value takes it out.
pub(crate) unsafe extern "C" fn ui_style_property(element: i32, name: *const u8, name_len: i32, value: *const u8, value_len: i32) {
    let (name, value) = unsafe { (utf8(name, name_len), utf8(value, value_len)) };
    ask(element, UiAsk::SetStyleProperty(&name, &value));
}

fn ask(element: i32, asked: UiAsk<'_>) -> Option<UiAnswer> {
    if element < 0 {
        return None;
    }
    with_services(|services| services.ui_ask(element as u32, asked))
}
