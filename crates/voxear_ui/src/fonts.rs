//! A project's own fonts, `.ttf` and `.otf` files, for text to draw in
//! beside the faces the system has.
//!
//! A font is known by what refers to it, its project path or its GUID, as a
//! sheet's `font-family: url("Fonts/Title.ttf")` names it. Naming one gives
//! a [`FontFace`] at once, even before its file is read; once whoever has
//! the project's files gives its data ([`register`]), every UI draws with
//! it from its next frame. Each is registered under a family name of its
//! own, so a font of the project's never stands in for one of the system's
//! of the same name.
//!
//! What draws a run of text, first to last, each tried for the letters the
//! one before has not got:
//!
//! 1. the element's own font, when its style names one;
//! 2. the fonts the game's language names, in order (see
//!    [`Ui::set_language_fonts`](crate::Ui::set_language_fonts)), as a
//!    font with Japanese's or Arabic's letters;
//! 3. Voxear's faces, Inter or Noto Sans (JetBrains Mono for `mono`);
//! 4. whatever the system has with the letter.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

use parley::fontique::{Blob, FontInfoOverride};

use crate::Family;

/// Voxear's own faces, best first, after any of the project's.
const SANS: &str = "Inter, \"Noto Sans\", \"DejaVu Sans\", sans-serif";
const MONO: &str = "\"JetBrains Mono\", \"Noto Sans Mono\", \"DejaVu Sans Mono\", monospace";

/// One of the project's fonts, by its place among those named so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontFace(u16);

/// A font named, and its data once given.
struct Slot {
    /// What named it first, for messages.
    reference: String,
    /// Its data, and the family name it is registered under, which changes
    /// when the data does.
    data: Option<(Arc<Vec<u8>>, String)>,
}

#[derive(Default)]
struct Faces {
    slots: Vec<Slot>,
    by_reference: HashMap<String, u16>,
    /// Counts each font given, for UIs to register what they have not.
    given: u64,
}

static FACES: LazyLock<Mutex<Faces>> = LazyLock::new(Mutex::default);

fn faces() -> MutexGuard<'static, Faces> {
    FACES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The font a reference names, its path or GUID: the same face each time,
/// whether or not its data has been given yet.
pub fn face(reference: &str) -> FontFace {
    let mut faces = faces();
    let key = reference.trim().to_string();
    if let Some(&at) = faces.by_reference.get(&key) {
        return FontFace(at);
    }
    let at = faces.slots.len().min(u16::MAX as usize) as u16;
    faces.slots.push(Slot { reference: key.clone(), data: None });
    faces.by_reference.insert(key, at);
    FontFace(at)
}

/// Gives a font's data, from its `.ttf` or `.otf` file, for whatever any
/// of `references` names: each of them then draws in it. Given again, as
/// after its file changed, the new data takes its place.
pub fn register(references: &[&str], data: Arc<Vec<u8>>) -> Option<FontFace> {
    let named: Vec<FontFace> = references.iter().filter(|r| !r.trim().is_empty()).map(|r| face(r)).collect();
    let first = *named.first()?;
    let mut faces = faces();
    faces.given += 1;
    let given = faces.given;
    for face in named {
        let slot = &mut faces.slots[face.0 as usize];
        if slot.data.as_ref().is_some_and(|(kept, _)| **kept == *data) {
            continue;
        }
        slot.data = Some((data.clone(), format!("voxear-font-{}-{given}", face.0)));
    }
    Some(first)
}

/// Forgets every font's data, as when a project closes; the faces named
/// stay, drawing in Voxear's faces until given again.
pub fn forget_all() {
    let mut faces = faces();
    faces.given += 1;
    for slot in &mut faces.slots {
        slot.data = None;
    }
}

impl FontFace {
    /// The family it draws as, once its data has been given.
    pub(crate) fn family(self) -> Option<String> {
        let faces = faces();
        faces.slots.get(self.0 as usize)?.data.as_ref().map(|(_, family)| family.clone())
    }

    /// What named it: its path or GUID.
    pub fn reference(self) -> String {
        faces().slots.get(self.0 as usize).map(|slot| slot.reference.clone()).unwrap_or_default()
    }

    /// Whether its data has been given.
    pub fn is_loaded(self) -> bool {
        self.family().is_some()
    }
}

/// How many fonts have been given, to tell a UI when to look again.
pub(crate) fn given() -> u64 {
    faces().given
}

/// The fonts given that `registered` has not yet, each with the family it
/// draws as.
pub(crate) fn unregistered(registered: &std::collections::HashSet<String>) -> Vec<(Arc<Vec<u8>>, String)> {
    let faces = faces();
    let fresh = faces.slots.iter().filter_map(|slot| slot.data.clone());
    fresh.filter(|(_, family)| !registered.contains(family)).collect()
}

/// Registers a font's data in a UI's fonts under `family`; whether it held
/// any face.
pub(crate) fn add_to(fonts: &mut parley::FontContext, data: &Arc<Vec<u8>>, family: &str) -> bool {
    let blob = Blob::new(Arc::new(data.as_ref().clone()));
    let info = FontInfoOverride { family_name: Some(family), ..FontInfoOverride::default() };
    !fonts.collection.register_fonts(blob, Some(info)).is_empty()
}

/// What a UI draws its text in, beyond each style's own font: the fonts
/// the game's language names, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FontStack {
    pub language: Vec<FontFace>,
}

impl FontStack {
    /// The families a style's text asks for, best first: its own font, the
    /// language's, then Voxear's (see the module's list).
    pub(crate) fn families(&self, family: Family) -> parley::FontFamily<'static> {
        parley::FontFamily::Source(std::borrow::Cow::Owned(self.names(family)))
    }

    /// The same, as a list of names.
    fn names(&self, family: Family) -> String {
        let own = match family {
            Family::Face(face) => Some(face),
            _ => None,
        };
        let mut names: Vec<String> = Vec::new();
        for face in own.into_iter().chain(self.language.iter().copied()) {
            if let Some(name) = face.family().filter(|name| !names.iter().any(|n| n.contains(name.as_str()))) {
                names.push(format!("\"{name}\""));
            }
        }
        names.push(match family {
            Family::Mono => MONO.to_string(),
            _ => SANS.to_string(),
        });
        names.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_is_named_before_it_is_read_and_drawn_once_given() {
        let face = face("Fonts/Named First.ttf");
        assert_eq!(face, super::face(" Fonts/Named First.ttf "), "the same face each time");
        assert!(!face.is_loaded());
        let stack = FontStack { language: vec![face] };
        assert_eq!(stack.names(Family::Sans), SANS, "Voxear's faces until it is read");
        let given = register(&["Fonts/Named First.ttf", "0123abcd"], Arc::new(vec![1, 2, 3])).unwrap();
        assert_eq!(given, face);
        let family = face.family().unwrap();
        assert!(family.starts_with("voxear-font-"), "a name of its own: {family}");
        assert!(super::face("0123abcd").family().is_some(), "its GUID names it too");
        assert_eq!(stack.names(Family::Mono), format!("\"{family}\", {MONO}"));
        let own = FontStack::default().names(Family::Face(face));
        assert_eq!(own, format!("\"{family}\", {SANS}"), "an element's own font comes first");
        register(&["Fonts/Named First.ttf"], Arc::new(vec![4, 5])).unwrap();
        assert_ne!(face.family().unwrap(), family, "new data, a new name, so it is registered afresh");
    }
}
