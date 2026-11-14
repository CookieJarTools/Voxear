//! Localized Text: a component that shows a string table key's text in an
//! element of its Obj's UI Panel, text in the world, or its Screen UI, in
//! the game's language, and shows it again whenever the language, the
//! tables or its values change. No script is needed.
//!
//! ```toml
//! [obj.ui_panel]
//! document = "UI/Sign.vui"
//!
//! [obj.localized_text]
//! key = "sign.tavern"         # the key, picked from the project's tables
//! element = "title"           # the element named so in the document
//! values = { owner = "Mara" } # what the key's text names, as {owner}
//! ```
//!
//! Without an element named, it shows in the document's first label. The
//! text is set on each frame it differs, so a script that changes the
//! element's text is overruled; a script changes the key or the values
//! instead (`LocalizedText.Key`, `SetValue`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use voxear_ecs::{Entity, World};
use voxear_localization::Localization;
use voxear_scene::Destroying;

use crate::{game_ui::GameUi, screen_ui::ScreenDocument, ui_panels::PanelDocument};

/// Shows a key's text in an element of its Obj's UI document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalizedText {
    /// The string table key, as `menu.play`.
    pub key: String,
    /// The element of the document it shows in, by its name; empty for the
    /// first label.
    pub element: String,
    /// What the key's text names, filled in by name, as `{count}`.
    pub values: BTreeMap<String, String>,
}

impl voxear_ecs::Component for LocalizedText {}

impl LocalizedText {
    /// Every setting, as a prefab instance's override merges them onto the
    /// prefab's: those that differ from `expected`.
    pub fn changes_from(&self, expected: Option<&Self>) -> toml::Table {
        let table = |text: &Self| {
            let mut table = toml::Table::new();
            table.insert("key".into(), text.key.clone().into());
            table.insert("element".into(), text.element.clone().into());
            let values = text.values.iter().map(|(name, value)| (name.clone(), toml::Value::from(value.clone())));
            table.insert("values".into(), toml::Value::Table(values.collect()));
            table
        };
        let mut changes = table(self);
        if let Some(expected) = expected.map(table) {
            changes.retain(|key, value| expected.get(key) != Some(value));
        }
        changes
    }

    /// Its text in the game's language now, its values filled in.
    pub fn text(&self, localization: &Localization) -> String {
        if self.values.is_empty() {
            return localization.text(&self.key).into_owned();
        }
        let values: Vec<(&str, &str)> =
            self.values.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect();
        localization.format(&self.key, &values)
    }
}

impl GameUi {
    /// Shows each Localized Text's key's text in its element, where it
    /// differs from what the element shows.
    pub fn show_localized_texts(&mut self, world: &World, localization: Option<&Localization>) {
        let Some(localization) = localization else { return };
        let shown: Vec<(Entity, LocalizedText)> = world
            .query_ref::<(Entity, &LocalizedText)>()
            .without::<Destroying>()
            .into_iter()
            .filter(|(_, text)| !text.key.is_empty())
            .map(|(entity, text)| (entity, text.clone()))
            .collect();
        for (entity, text) in shown {
            let panel = world.get::<PanelDocument>(entity).map(|document| document.0);
            let Some(document) = panel.or_else(|| world.get::<ScreenDocument>(entity).map(|document| document.0))
            else {
                continue;
            };
            let found = match text.element.as_str() {
                "" => self.first_label(document),
                name => self.find(document, name),
            };
            let Some(element) = found else { continue };
            let words = text.text(localization);
            if self.text(element).as_deref() != Some(words.as_str()) {
                // A text that starts with @ is not a key, so it is written as markup writes a plain @.
                let plain = if words.starts_with('@') { format!("@{words}") } else { words };
                self.set_text(element, &plain);
            }
        }
    }
}

/// Sets what answers `LocalizedText` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::LocalizedText, values);
    voxear_scripting::set_kind_texts(voxear_scripting::ComponentKind::LocalizedText, texts);
}

// What `values` and `texts` read or write, as `LocalizedText` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const KEY: i32 = 3;
const SET_KEY: i32 = 4;
const ELEMENT: i32 = 5;
const SET_ELEMENT: i32 = 6;
/// A value by name, read; and set from `name\nvalue`, or taken out from
/// the name alone.
const VALUE: i32 = 7;
const SET_VALUE: i32 = 8;
/// Every value taken out.
const CLEAR_VALUES: i32 = 9;
/// Its text as shown now, in the game's language.
const SHOWN: i32 = 10;

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => Some([f32::from(u8::from(world.has::<LocalizedText>(entity))), 0.0, 0.0, 0.0]),
        (ADD, Some(_)) => {
            if !world.has::<LocalizedText>(entity) {
                world.insert(entity, LocalizedText::default());
            }
            Some([0.0; 4])
        }
        (REMOVE, Some(_)) => world.remove::<LocalizedText>(entity).map(|_| [0.0; 4]),
        (CLEAR_VALUES, Some(_)) => {
            world.get_mut::<LocalizedText>(entity)?.values.clear();
            Some([0.0; 4])
        }
        _ => None,
    }
}

fn texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    if !world.is_alive(entity) {
        return None;
    }
    if what == SHOWN {
        let shown = world.get::<LocalizedText>(entity)?;
        return Some(world.resource::<Localization>().map_or_else(|| shown.key.clone(), |words| shown.text(words)));
    }
    let localized = world.get_mut::<LocalizedText>(entity)?;
    match what {
        KEY => Some(localized.key.clone()),
        SET_KEY => {
            localized.key = text.trim().to_string();
            Some(String::new())
        }
        ELEMENT => Some(localized.element.clone()),
        SET_ELEMENT => {
            localized.element = text.trim().to_string();
            Some(String::new())
        }
        VALUE => Some(localized.values.get(text).cloned().unwrap_or_default()),
        SET_VALUE => {
            match text.split_once('\n') {
                Some((name, value)) => drop(localized.values.insert(name.to_string(), value.to_string())),
                None => drop(localized.values.remove(text)),
            }
            Some(String::new())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_localized_text_fills_in_its_values_and_saves_every_setting_it_changes() {
        let mut localization = Localization::new(&["en".into()]);
        let table =
            "[[entry]]\nkey = \"loot\"\nen = \"{player} found {count, plural, one {# coin} other {# coins}}\"\n";
        localization.add_table(&voxear_localization::StringTable::parse(table).unwrap());
        let mut text = LocalizedText { key: "loot".into(), ..LocalizedText::default() };
        text.values.insert("player".into(), "Mara".into());
        text.values.insert("count".into(), "1".into());
        assert_eq!(text.text(&localization), "Mara found 1 coin");
        text.values.insert("count".into(), "3".into());
        assert_eq!(text.text(&localization), "Mara found 3 coins");

        let written = toml::to_string(&text).unwrap();
        assert_eq!(toml::from_str::<LocalizedText>(&written).unwrap(), text, "{written}");
        let read: LocalizedText = toml::from_str("key = \"menu.play\"").unwrap();
        assert_eq!((read.element.as_str(), read.values.len()), ("", 0), "left out, the first label and no values");
        let plain = LocalizedText { key: "loot".into(), ..LocalizedText::default() };
        let changes = plain.changes_from(Some(&text));
        assert_eq!(changes.keys().collect::<Vec<_>>(), ["values"], "only what differs, even when emptied");
    }
}
