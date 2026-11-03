//! The language Steam says the game is in, as the player set it for this
//! game in their Steam library, which a game sold on Steam starts in
//! before the system's.
//!
//! Voxear does not ship Steamworks, which each developer has from Valve
//! under their own agreement. A game on Steam puts the SDK's redistributable library beside
//! its executable (`steam_api64.dll` on Windows, `libsteam_api.so` on
//! Linux, `libsteam_api.dylib` on macOS, from the SDK's
//! `redistributable_bin`), or names it with `VOXEAR_STEAMWORKS`. Voxear
//! loads it while the game starts, and with Steam running asks it the
//! game's language. With no library, or no Steam, there is none, and the
//! system's language is used as before.

use std::{
    ffi::{CStr, c_char, c_void},
    path::PathBuf,
};

/// The redistributable's name on this system.
#[cfg(windows)]
const LIBRARY: &str = "steam_api64.dll";
#[cfg(target_os = "macos")]
const LIBRARY: &str = "libsteam_api.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
const LIBRARY: &str = "libsteam_api.so";

/// Steam's names for its languages, and their codes.
const LANGUAGES: [(&str, &str); 30] = [
    ("arabic", "ar"),
    ("bulgarian", "bg"),
    ("schinese", "zh-Hans"),
    ("tchinese", "zh-Hant"),
    ("czech", "cs"),
    ("danish", "da"),
    ("dutch", "nl"),
    ("english", "en"),
    ("finnish", "fi"),
    ("french", "fr"),
    ("german", "de"),
    ("greek", "el"),
    ("hungarian", "hu"),
    ("indonesian", "id"),
    ("italian", "it"),
    ("japanese", "ja"),
    ("koreana", "ko"),
    ("norwegian", "no"),
    ("polish", "pl"),
    ("portuguese", "pt"),
    ("brazilian", "pt-BR"),
    ("romanian", "ro"),
    ("russian", "ru"),
    ("spanish", "es"),
    ("latam", "es-419"),
    ("swedish", "sv"),
    ("thai", "th"),
    ("turkish", "tr"),
    ("ukrainian", "uk"),
    ("vietnamese", "vi"),
];

/// The game's language as Steam has it, by its code; none without the
/// developer's Steamworks library or without Steam running.
pub fn steam_language() -> Option<String> {
    let path = library()?;
    // SAFETY: the library is Steam's own redistributable, which the developer put beside the game; its flat API
    // is C, and each call is given what Steamworks' steam_api_flat.h says it takes.
    unsafe {
        let library = libloading::Library::new(&path).ok()?;
        if !initialised(&library) {
            tracing::info!("{}: Steam is not running for this game; the system's language is used", path.display());
            return None;
        }
        let apps: libloading::Symbol<unsafe extern "C" fn() -> *mut c_void> =
            library.get(b"SteamAPI_SteamApps_v008\0").ok()?;
        let language: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> *const c_char> =
            library.get(b"SteamAPI_ISteamApps_GetCurrentGameLanguage\0").ok()?;
        let apps = apps();
        let named = (!apps.is_null()).then(|| language(apps)).filter(|named| !named.is_null())?;
        let code = code_of(&CStr::from_ptr(named).to_string_lossy());
        // Steam stays initialised for the game's life, as it is meant to be: the library is kept loaded.
        std::mem::forget(library);
        code
    }
}

/// Starts Steam's API: `SteamAPI_InitFlat` from Steamworks 1.59 on, or
/// `SteamAPI_Init` before.
unsafe fn initialised(library: &libloading::Library) -> bool {
    unsafe {
        if let Ok(init) = library.get::<unsafe extern "C" fn(*mut [c_char; 1024]) -> i32>(b"SteamAPI_InitFlat\0") {
            let mut message = [0 as c_char; 1024];
            return init(&mut message) == 0;
        }
        library.get::<unsafe extern "C" fn() -> bool>(b"SteamAPI_Init\0").is_ok_and(|init| init())
    }
}

/// Where the developer's Steamworks library is: named by
/// `VOXEAR_STEAMWORKS`, or beside the game's executable.
fn library() -> Option<PathBuf> {
    let named = std::env::var_os("VOXEAR_STEAMWORKS").map(PathBuf::from);
    let beside = || std::env::current_exe().ok()?.parent().map(|folder| folder.join(LIBRARY));
    named.or_else(beside).filter(|path| path.is_file())
}

/// A language's code by Steam's name for it.
fn code_of(steam: &str) -> Option<String> {
    LANGUAGES.iter().find(|(name, _)| *name == steam.trim()).map(|(_, code)| code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steams_names_for_languages_are_read_as_codes() {
        assert_eq!(code_of("schinese").as_deref(), Some("zh-Hans"));
        assert_eq!(code_of("brazilian").as_deref(), Some("pt-BR"));
        assert_eq!(code_of("koreana").as_deref(), Some("ko"));
        assert_eq!(code_of("klingon"), None);
    }
}
