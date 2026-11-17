//! A setting of Project Settings or Preferences, described once: the
//! category and section it shows in, its label and tooltip, what it holds
//! and the range it may take, and whether it is an advanced one, folded away
//! under its section. Both windows are built from these (see
//! `settings_window`), the windows' search and the command palette find
//! them, and `vox editor call` lists them with their values, so a tool can
//! use every setting the person can.
//!
//! A value goes between them as JSON: a toggle as a bool, a choice as its
//! option's label, a number as a number, a vector as an array of numbers, a
//! colour as `#rrggbb`, a folder or an asset as its path or null, text as a
//! string, and a list as an array of strings.

use serde_json::{Value as Json, json};
use voxear_assets::AssetKind;

/// What a setting holds, and so how it is shown and changed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Toggle,
    /// One of these, each as shown and as its file writes it.
    Choice(&'static [(&'static str, &'static str)]),
    /// A number from `min` to `max`, in `unit`, whole where `whole`, with a
    /// slider beside its field where `slider`.
    Number {
        min: f64,
        max: f64,
        unit: &'static str,
        whole: bool,
        slider: bool,
    },
    /// So many numbers together, as a gravity's x, y and z, in `unit`.
    Vector {
        count: usize,
        unit: &'static str,
    },
    /// A colour, picked from its swatch.
    Color,
    /// A folder on this machine, or none.
    Folder,
    /// Words typed in, as a name.
    Text,
    /// An asset of this kind, picked from the project's, or none.
    Asset(AssetKind),
    /// Words, one a row, as file patterns.
    Words,
    /// Some of these, one a row, as languages.
    Picks(&'static [(&'static str, &'static str)]),
    /// Shown, not changed here.
    Info,
    /// A part of the window of its own, as the collision matrix: the window
    /// gives it a place, and the editor fills it.
    Part,
    /// A button that does something, with these words on it.
    Button(&'static str),
}

/// One setting (see the module's notes). `key` is what it is kept and found
/// by: for the project's, its table and key in `voxear.toml`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setting {
    pub key: &'static str,
    pub category: &'static str,
    pub section: &'static str,
    pub label: &'static str,
    /// What it does, what more or less of it costs or changes, and when it
    /// takes effect; its default is added after.
    pub tip: &'static str,
    pub kind: Kind,
    /// Rarely wanted: folded away under its section's Advanced.
    pub advanced: bool,
}

impl Setting {
    pub const fn new(
        (category, section): (&'static str, &'static str),
        key: &'static str,
        label: &'static str,
        kind: Kind,
        tip: &'static str,
    ) -> Self {
        Self { key, category, section, label, tip, kind, advanced: false }
    }

    /// The same, folded away under its section's Advanced.
    pub const fn advanced(self) -> Self {
        Self { advanced: true, ..self }
    }

    /// Whether it holds a value that can be set: not information, a part
    /// of its own or a button.
    pub fn settable(&self) -> bool {
        !matches!(self.kind, Kind::Info | Kind::Part | Kind::Button(_))
    }

    /// Whether every word of `query` is in its label, section, category,
    /// key or tooltip, any case.
    pub fn found(&self, query: &str) -> bool {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return true;
        }
        let haystack =
            [self.label, self.section, self.category, self.key, self.tip].join(" ").to_lowercase().replace('_', " ");
        words.iter().all(|word| haystack.contains(word.as_str()))
    }
}

/// A category of a settings window, as its list shows it: its name, what
/// is in it, for its tooltip, and its page of the manual.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Category {
    pub name: &'static str,
    pub about: &'static str,
    pub manual: &'static str,
}

/// A line of help under a section's title, where one really helps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionHelp {
    pub category: &'static str,
    pub section: &'static str,
    pub help: &'static str,
}

/// The value `value` would be for `setting`, as it is kept: a choice by its
/// label whichever way it was named, a whole number rounded. Refused, with
/// what it may be, when it is not a value for it.
pub fn check(setting: &Setting, value: &Json) -> Result<Json, String> {
    let wrong = |may: &str| format!("{value} is not a value for {}: it is {may}", setting.label);
    match setting.kind {
        Kind::Toggle => value.as_bool().map(Json::Bool).ok_or_else(|| wrong("true or false")),
        Kind::Choice(options) => {
            let word = value.as_str().ok_or_else(|| wrong(&choices(options)))?;
            let found = options
                .iter()
                .find(|(label, kept)| label.eq_ignore_ascii_case(word) || kept.eq_ignore_ascii_case(word));
            found.map(|(label, _)| json!(label)).ok_or_else(|| wrong(&choices(options)))
        }
        Kind::Number { min, max, whole, .. } => {
            let range = format!("a number from {} to {}", number_text(min), number_text(max));
            let number = value.as_f64().filter(|n| n.is_finite()).ok_or_else(|| wrong(&range))?;
            let number = if whole { number.round() } else { number };
            if number < min || number > max {
                return Err(wrong(&range));
            }
            Ok(if whole { json!(number as i64) } else { json!(number) })
        }
        Kind::Vector { count, .. } => {
            let numbers: Option<Vec<f64>> = value
                .as_array()
                .filter(|items| items.len() == count)
                .and_then(|items| items.iter().map(|item| item.as_f64().filter(|n| n.is_finite())).collect());
            numbers.map(|numbers| json!(numbers)).ok_or_else(|| wrong(&format!("{count} numbers")))
        }
        Kind::Color => {
            let hex = value.as_str().and_then(|text| text.strip_prefix('#')).filter(|hex| hex.len() == 6);
            let parsed = hex.and_then(|hex| u32::from_str_radix(hex, 16).ok());
            parsed.map(|colour| json!(format!("#{colour:06x}"))).ok_or_else(|| wrong("a colour as #rrggbb"))
        }
        Kind::Folder | Kind::Asset(_) => match value {
            Json::Null => Ok(Json::Null),
            Json::String(_) => Ok(value.clone()),
            _ => Err(wrong("a path, or null for none")),
        },
        Kind::Text => value.as_str().map(|text| json!(text.trim())).ok_or_else(|| wrong("words")),
        Kind::Words => {
            let words: Option<Vec<String>> =
                value.as_array().and_then(|items| items.iter().map(|item| item.as_str().map(str::to_string)).collect());
            words.map(|words| json!(words)).ok_or_else(|| wrong("a list of words"))
        }
        Kind::Picks(options) => {
            let items = value.as_array().ok_or_else(|| wrong(&format!("a list of {}", choices(options))))?;
            let mut picked = Vec::new();
            for item in items {
                let word = item.as_str().unwrap_or_default();
                let found = options
                    .iter()
                    .find(|(label, kept)| label.eq_ignore_ascii_case(word) || kept.eq_ignore_ascii_case(word));
                let (label, _) = found.ok_or_else(|| wrong(&format!("a list of {}", choices(options))))?;
                if !picked.contains(label) {
                    picked.push(*label);
                }
            }
            Ok(json!(picked))
        }
        Kind::Info | Kind::Part | Kind::Button(_) => Err(format!("{} is not changed by a value", setting.label)),
    }
}

/// A choice's options as a list in words.
fn choices(options: &[(&str, &str)]) -> String {
    options.iter().map(|(label, _)| *label).collect::<Vec<_>>().join(", ")
}

/// The word a choice's file writes for its option labelled `label`.
pub fn kept_word(options: &[(&'static str, &'static str)], label: &str) -> Option<&'static str> {
    options
        .iter()
        .find(|(shown, kept)| shown.eq_ignore_ascii_case(label) || kept.eq_ignore_ascii_case(label))
        .map(|(_, kept)| *kept)
}

/// The label of a choice's option its file writes as `word`.
pub fn shown_label(options: &[(&'static str, &'static str)], word: &str) -> Option<&'static str> {
    options
        .iter()
        .find(|(shown, kept)| kept.eq_ignore_ascii_case(word) || shown.eq_ignore_ascii_case(word))
        .map(|(shown, _)| *shown)
}

/// A number as few figures as say it: "0.25", "8192".
pub fn number_text(number: f64) -> String {
    let rounded = (number * 1000.0).round() / 1000.0;
    let text = format!("{:.3}", rounded as f32);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A value as words: "On", "0.25 m", "Parametric", "None".
pub fn describe(setting: &Setting, value: &Json) -> String {
    let unit = |text: String, unit: &str| if unit.is_empty() { text } else { format!("{text} {unit}") };
    match (setting.kind, value) {
        (_, Json::Null) => "None".into(),
        (Kind::Toggle, Json::Bool(on)) => if *on { "On" } else { "Off" }.into(),
        (Kind::Number { unit: u, .. }, Json::Number(n)) => unit(number_text(n.as_f64().unwrap_or_default()), u),
        (Kind::Vector { unit: u, .. }, Json::Array(items)) => {
            let numbers: Vec<String> = items.iter().map(|n| number_text(n.as_f64().unwrap_or_default())).collect();
            unit(numbers.join(", "), u)
        }
        (Kind::Words | Kind::Picks(_), Json::Array(items)) if items.is_empty() => "None".into(),
        (Kind::Words | Kind::Picks(_), Json::Array(items)) => {
            items.iter().filter_map(Json::as_str).collect::<Vec<_>>().join(", ")
        }
        (_, Json::String(text)) => text.clone(),
        (_, other) => other.to_string(),
    }
}

/// Whether two values are the same, numbers as closely as they are kept.
pub fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(a), Json::Number(b)) => {
            (a.as_f64().unwrap_or_default() as f32) == (b.as_f64().unwrap_or_default() as f32)
        }
        (Json::Array(a), Json::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b)),
        _ => a == b,
    }
}

/// The tooltip a setting's label shows: what it does, then its default.
pub fn tooltip(setting: &Setting, default: Option<&Json>) -> String {
    match default.filter(|_| setting.settable()) {
        Some(default) => format!("{}\n\nDefault: {}", setting.tip, describe(setting, default)),
        None => setting.tip.to_string(),
    }
}

/// A setting as `vox editor call` lists it: where it shows, what it is
/// called and does, what it holds and may be, its value and its default.
/// `title` and `group` are the label and section by their older names.
pub fn listed(setting: &Setting, value: &Json, default: &Json) -> Json {
    let kind = match setting.kind {
        Kind::Toggle => "toggle",
        Kind::Choice(_) => "choice",
        Kind::Number { .. } => "number",
        Kind::Vector { .. } => "vector",
        Kind::Color => "color",
        Kind::Folder => "folder",
        Kind::Text => "text",
        Kind::Asset(_) => "asset",
        Kind::Words => "list",
        Kind::Picks(_) => "choices",
        Kind::Info => "info",
        Kind::Part => "part",
        Kind::Button(_) => "button",
    };
    let mut shown = json!({
        "key": setting.key,
        "category": setting.category,
        "section": setting.section,
        "label": setting.label,
        "title": setting.label,
        "group": setting.section,
        "tooltip": setting.tip,
        "type": kind,
        "advanced": setting.advanced,
    });
    if setting.settable() {
        shown["value"] = value.clone();
        shown["default"] = default.clone();
        shown["changed"] = json!(!same(value, default));
    } else if matches!(setting.kind, Kind::Info) {
        shown["value"] = value.clone();
    }
    match setting.kind {
        Kind::Choice(options) | Kind::Picks(options) => {
            shown["options"] = json!(options.iter().map(|(label, _)| *label).collect::<Vec<_>>());
        }
        Kind::Number { min, max, unit, whole, .. } => {
            shown["min"] = json!(min);
            shown["max"] = json!(max);
            shown["unit"] = json!(unit);
            shown["whole"] = json!(whole);
        }
        Kind::Vector { count, unit } => {
            shown["count"] = json!(count);
            shown["unit"] = json!(unit);
        }
        Kind::Asset(kind) => shown["asset"] = json!(format!("{kind:?}")),
        _ => {}
    }
    shown
}

/// The settings a search for `query` finds, best first: those whose label
/// matches it best, then the rest whose words have it somewhere.
pub fn search<'a>(settings: &'a [Setting], query: &str) -> Vec<&'a Setting> {
    let mut found: Vec<(i32, usize, &Setting)> = settings
        .iter()
        .enumerate()
        .filter(|(_, setting)| setting.found(query))
        .map(|(at, setting)| (crate::palette::score(query, setting.label).unwrap_or(i32::MIN), at, setting))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, _, setting)| setting).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: Setting = Setting::new(("Audio", "Sound"), "audio.reverb", "Reverb", Kind::Toggle, "Rooms ring.");
    const MODE: Setting = Setting::new(
        ("Audio", "Sound"),
        "audio.reverb_mode",
        "Reverb mode",
        Kind::Choice(&[("Parametric", "parametric"), ("Convolution", "convolution")]),
        "How rooms ring.",
    );
    const PORT: Setting = Setting::new(
        ("Network", "Session"),
        "network.port",
        "Port",
        Kind::Number { min: 1.0, max: 65535.0, unit: "", whole: true, slider: false },
        "The port a server listens on.",
    );

    #[test]
    fn values_are_checked_against_what_a_setting_holds() {
        assert_eq!(check(&ON, &json!(true)), Ok(json!(true)));
        assert!(check(&ON, &json!("yes")).is_err());
        assert_eq!(check(&MODE, &json!("convolution")), Ok(json!("Convolution")), "by its word or its label");
        assert!(check(&MODE, &json!("Echo")).unwrap_err().contains("Parametric, Convolution"), "says what it may be");
        assert_eq!(check(&PORT, &json!(7777.4)), Ok(json!(7777)), "whole numbers round");
        assert!(check(&PORT, &json!(70000)).unwrap_err().contains("from 1 to 65535"));
        assert_eq!(describe(&PORT, &json!(7777)), "7777");
        assert!(same(&json!(0.02), &json!(0.019999999552965164)), "an f32 read back is the number written");
        assert_eq!(tooltip(&ON, Some(&json!(false))), "Rooms ring.\n\nDefault: Off");
    }

    #[test]
    fn a_search_finds_a_setting_by_any_of_its_words_label_first() {
        let settings = [ON, MODE, PORT];
        let found: Vec<&str> = search(&settings, "reverb").iter().map(|s| s.key).collect();
        assert_eq!(found, ["audio.reverb", "audio.reverb_mode"], "the closest label first");
        let by_tip: Vec<&str> = search(&settings, "server listens").iter().map(|s| s.key).collect();
        assert_eq!(by_tip, ["network.port"], "words of its tooltip count");
        assert_eq!(search(&settings, "audio ring").len(), 2, "and its category");
        let listed = listed(&PORT, &json!(7777), &json!(7777));
        assert_eq!(
            (listed["type"].as_str(), listed["max"].as_f64(), listed["changed"].as_bool()),
            (Some("number"), Some(65535.0), Some(false))
        );
    }
}
