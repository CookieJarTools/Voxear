//! The game's text for C#: a key's text in the language the game is in,
//! with values filled in, and the language, read and set, for
//! `Localization` (see `voxear_localization`). Without a world, as in
//! tests, a key is its own text.

use voxear_localization::Localization;

use crate::interop::{utf8, with_world};

/// Writes `text` into the buffer when it fits; returns its length in bytes
/// either way, for a caller to ask again with room for it.
fn write(text: &str, buffer: *mut u8, capacity: i32) -> i32 {
    if !buffer.is_null() && text.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len()) };
    }
    text.len() as i32
}

/// A key's text, with the values named in `values` filled in: each name
/// and its value one after the other, each ended by a zero byte.
pub(crate) unsafe extern "C" fn text_get(
    key: *const u8,
    len: i32,
    values: *const u8,
    values_len: i32,
    buffer: *mut u8,
    capacity: i32,
) -> i32 {
    let key = unsafe { utf8(key, len) };
    let values = unsafe { utf8(values, values_len) };
    let parts: Vec<&str> = values.split_terminator('\0').collect();
    let named: Vec<(&str, &str)> = parts.as_chunks::<2>().0.iter().map(|&[name, value]| (name, value)).collect();
    let text = with_world(|world| {
        let localization = world.resource::<Localization>()?;
        Some(if named.is_empty() { localization.text(&key).into_owned() } else { localization.format(&key, &named) })
    });
    let text = text.unwrap_or_else(|| voxear_localization::fill(&key, &named));
    write(&text, buffer, capacity)
}

/// The language the game is in, put first in the project's nearest to
/// `code` when `set`; returns the length of its code. Setting one the
/// project has nothing near leaves it as it was.
pub(crate) unsafe extern "C" fn text_language(
    set: i32,
    code: *const u8,
    len: i32,
    buffer: *mut u8,
    capacity: i32,
) -> i32 {
    let code = unsafe { utf8(code, len) };
    let language = with_world(|world| {
        let localization = world.resource_mut::<Localization>()?;
        if set != 0 {
            localization.set_language(&code);
        }
        Some(localization.language().to_string())
    });
    write(&language.unwrap_or_else(|| "en".to_string()), buffer, capacity)
}

/// The project's languages, its own first, one a line.
pub(crate) unsafe extern "C" fn text_languages(buffer: *mut u8, capacity: i32) -> i32 {
    let languages = with_world(|world| Some(world.resource::<Localization>()?.languages().join("\n")));
    write(&languages.unwrap_or_else(|| "en".to_string()), buffer, capacity)
}

/// Counts each change of the game's language or text, for text shown from
/// it to tell when to look again.
pub(crate) unsafe extern "C" fn text_version() -> u64 {
    with_world(|world| Some(world.resource::<Localization>()?.version())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use voxear_ecs::World;
    use voxear_localization::StringTable;

    use super::*;
    use crate::interop::WorldScope;

    fn read(call: impl Fn(*mut u8, i32) -> i32) -> String {
        let mut buffer = vec![0u8; 64];
        let length = call(buffer.as_mut_ptr(), buffer.len() as i32);
        String::from_utf8(buffer[..length as usize].to_vec()).unwrap()
    }

    #[test]
    fn scripts_read_a_keys_text_and_put_the_game_in_another_language() {
        let mut world = World::new();
        let mut localization = Localization::new(&["en".into(), "fr".into()]);
        let table = "[[entry]]\nkey = \"loot\"\nen = \"{player} found {count} coins\"\nfr = \"{player} a trouvé {count} pièces\"\n";
        localization.add_table(&StringTable::parse(table).unwrap());
        world.insert_resource(localization);
        let _scope = WorldScope::enter(&mut world);
        let (key, values) = ("loot", "player\0Ana\0count\u{0}3\0");
        let get =
            || read(|out, room| unsafe { text_get(key.as_ptr(), 4, values.as_ptr(), values.len() as i32, out, room) });
        assert_eq!(get(), "Ana found 3 coins");
        let before = unsafe { text_version() };
        let set = read(|out, room| unsafe { text_language(1, "fr-CA".as_ptr(), 5, out, room) });
        assert_eq!((set.as_str(), get().as_str()), ("fr", "Ana a trouvé 3 pièces"));
        assert!(unsafe { text_version() } > before);
        assert_eq!(read(|out, room| unsafe { text_languages(out, room) }), "en\nfr");
        let small = unsafe { text_get(key.as_ptr(), 4, values.as_ptr(), values.len() as i32, std::ptr::null_mut(), 0) };
        assert_eq!(small as usize, "Ana a trouvé 3 pièces".len(), "the length, to ask again with room");
    }
}
