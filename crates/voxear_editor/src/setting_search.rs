//! Finding a setting by what it does, for when its name is not known: one
//! search over every setting the editor has (Project Settings, the
//! Lighting panel, Preferences, the components of the open level's Objs,
//! and import settings) that matches rough words against each setting's
//! name, group and tooltip, and words that mean the same (shimmer, sparkle,
//! flicker), not only the name. Results are ranked by how well each
//! matched, with the words it matched and the line of its tooltip that
//! matched. The palette (Ctrl+K) and each settings window's own search use
//! it, and `vox editor call settings.find`; picking one opens its page or
//! panel at it (see `Editor::show_found`). A setting hidden for now, as one
//! of a module the project is without or one the Lighting panel shows only
//! while another is on, is still found, saying what turns it on.

use voxear_ecs::Entity;
use voxear_engine::{Game, modules::Module};
use voxear_scene::{EditorOnly, Name};

use crate::setting_info::Setting;

/// Where a setting is, to open it there.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    /// Project Settings, by key.
    Project(&'static str),
    /// Preferences, by key.
    Preference(&'static str),
    /// The Lighting panel, by the setting's path in a `.lighting` file and its tab.
    Lighting { path: String, group: &'static str, title: &'static str },
    /// A component's field on an Obj of the open level, by the component's title and the field's label.
    Component { entity: Entity, component: String, field: String },
    /// An asset kind's import settings, by the kind's name and the setting's label.
    Import { kind: &'static str, label: &'static str },
}

impl Place {
    /// The key a result is found again by, as `settings.show` takes it:
    /// a Project Settings or Preferences key as it is, and the others with
    /// what they are first (`lighting:post.bloom.intensity`).
    pub fn key(&self) -> String {
        match self {
            Self::Project(key) | Self::Preference(key) => key.to_string(),
            Self::Lighting { path, .. } => format!("lighting:{path}"),
            Self::Component { entity, component, field } => {
                format!("component:{}:{component}:{field}", entity.to_bits())
            }
            Self::Import { kind, label } => format!("import:{kind}:{label}"),
        }
    }
}

/// A setting as the search knows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub place: Place,
    pub label: String,
    /// Where it is, as a result shows it: `Project Settings · Physics ▸ General`.
    pub group: String,
    pub tip: String,
    /// What turns it on, while it is hidden.
    pub hidden: Option<String>,
}

/// A setting found: its entry, how well it matched, the words of its label
/// and tooltip that matched, and the line of its tooltip that did.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub entry: Entry,
    pub score: i32,
    pub words: Vec<String>,
    pub line: String,
}

/// Words that mean much the same in what people say of a picture, a sound
/// or a game. A word typed matches each word of its group too.
const SAME: &[&[&str]] = &[
    &["shimmer", "sparkle", "flicker", "aliasing", "jaggies", "jagged", "crawl", "anti-aliasing", "taa", "fxaa"],
    &["blur", "blurry", "soft", "smear", "smudge", "focus"],
    &["dark", "darker", "dim", "black", "shadow", "shade", "exposure"],
    &["bright", "brighter", "brightness", "exposure", "glow", "bloom", "light"],
    &["fog", "haze", "mist", "murk", "volumetric"],
    &["fps", "framerate", "frame", "slow", "lag", "laggy", "performance", "cost", "budget", "speed"],
    &["stutter", "hitch", "jitter", "jerky", "judder", "smooth"],
    &["tear", "tearing", "vsync"],
    &["loud", "quiet", "volume", "mute"],
    &["echo", "reverb", "ring", "room"],
    &["pitch", "doppler"],
    &["far", "distance", "draw", "range", "view", "pop", "popping", "lod", "detail"],
    &["blocky", "pixel", "pixelated", "voxel", "cube", "blocks"],
    &["gravity", "weight", "pull"],
    &["bounce", "bouncy", "bounciness", "restitution"],
    &["slippery", "friction", "grip", "slide"],
    &["drift", "deadzone", "dead", "stick", "joystick", "gamepad", "controller"],
    &["trigger", "press", "button"],
    &["wind", "sway", "breeze", "bend"],
    &["grass", "plants", "foliage", "flowers", "bushes", "vegetation"],
    &["water", "sea", "ocean", "lake", "river", "waves"],
    &["sky", "clouds", "sun", "weather", "rain", "storm"],
    &["colour", "color", "tint", "hue", "saturation", "grading"],
    &["sharp", "sharpen", "crisp", "upscale", "upscaler", "resolution", "scale"],
    &["hdr", "tonemap", "tonemapper", "highlights"],
    &["time", "slow-motion", "timescale", "step", "tick"],
    &["network", "online", "multiplayer", "server", "players", "lag", "latency", "bandwidth"],
    &["language", "translation", "localization", "font"],
    &["reflection", "reflections", "mirror", "shiny", "gloss", "glossy"],
    &["gi", "bounce", "indirect", "ambient", "occlusion", "ao"],
];

/// How much a word of each part of a setting counts, the word typed and one
/// that means the same.
const LABEL: (i32, i32) = (12, 7);
const GROUP: (i32, i32) = (5, 3);
const TIP: (i32, i32) = (3, 2);

/// Every setting there is: Project Settings', Preferences', the Lighting
/// panel's, the import settings', and the components' of the open level's
/// Objs, each component's fields once, on the first Obj that has it.
pub fn everything(game: &Game, types: &voxear_engine::script_types::ScriptTypes) -> Vec<Entry> {
    let mut entries = settings_of(crate::project_settings::settings(), "Project Settings", Place::Project, |s| {
        hidden_by_module(game, s)
    });
    entries.extend(settings_of(crate::settings::PREFERENCES, "Preferences", Place::Preference, |_| None));
    entries.extend(crate::lighting_panel::described().into_iter().map(|(path, title, group, help, when)| Entry {
        place: Place::Lighting { path, group, title },
        label: title.to_string(),
        group: format!("Lighting · {group}"),
        tip: help.to_string(),
        hidden: when,
    }));
    entries.extend(crate::import_fields::DESCRIBED.iter().map(|&(kind, label, tip)| Entry {
        place: Place::Import { kind, label },
        label: label.to_string(),
        group: format!("Import Settings · {kind}"),
        tip: tip.to_string(),
        hidden: None,
    }));
    entries.extend(components(game, types));
    entries
}

/// A window's settings as entries.
fn settings_of(
    settings: &'static [Setting],
    window: &str,
    place: fn(&'static str) -> Place,
    hidden: impl Fn(&Setting) -> Option<String>,
) -> Vec<Entry> {
    let shown = settings.iter().filter(|s| s.settable() || matches!(s.kind, crate::setting_info::Kind::Button(_)));
    shown
        .map(|setting| Entry {
            place: place(setting.key),
            label: setting.label.to_string(),
            group: format!("{window} · {} ▸ {}", setting.category, setting.section),
            tip: setting.tip.to_string(),
            hidden: hidden(setting),
        })
        .collect()
}

/// The module a Project Settings setting does nothing without, said as
/// what turns it on, while the project is without it.
fn hidden_by_module(game: &Game, setting: &Setting) -> Option<String> {
    let module = match setting.key {
        "physics.gravity_2d" => Module::Physics2D,
        "audio.steam_audio" => Module::SteamAudio,
        key if key.starts_with("network.") => Module::Networking,
        key if key.starts_with("foliage.") => Module::Vegetation,
        key if key.starts_with("water_trails.") => Module::Water,
        key if key.starts_with("world.") => Module::Terrain,
        _ => return None,
    };
    (!game.project.modules().has(module)).then(|| format!("Turned on by the {} module", module.title()))
}

/// The fields of the components on the open level's Objs, each
/// component's once, on the first Obj that has it.
fn components(game: &Game, types: &voxear_engine::script_types::ScriptTypes) -> Vec<Entry> {
    const MOST_OBJS: usize = 2000;
    let world = &game.engine.world;
    let modules = game.project.modules();
    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::new();
    let objs = world
        .query_ref::<(Entity, &Name)>()
        .into_iter()
        .filter(|(entity, _)| world.get::<EditorOnly>(*entity).is_none());
    for (entity, name) in objs.take(MOST_OBJS) {
        for part in crate::props::parts(world, entity) {
            let component = part.title().to_string();
            if !seen.insert(component.clone()) {
                continue;
            }
            let hidden =
                part.module().filter(|&m| !modules.has(m)).map(|m| format!("Turned on by the {} module", m.title()));
            for prop in crate::props::props(world, entity, &part, types) {
                let field = crate::props::label(&prop);
                let tip = crate::inspector::field_tooltip(types, &prop).unwrap_or_default();
                entries.push(Entry {
                    place: Place::Component { entity, component: component.clone(), field: field.clone() },
                    label: field,
                    group: format!("Inspector · {component} on {}", name.0),
                    tip,
                    hidden: hidden.clone(),
                });
            }
        }
    }
    entries
}

/// A setting of Project Settings or Preferences as an entry, for a
/// window's own search.
pub fn entry_of(setting: &'static Setting) -> Entry {
    Entry {
        place: Place::Project(setting.key),
        label: setting.label.to_string(),
        group: format!("{} ▸ {}", setting.category, setting.section),
        tip: setting.tip.to_string(),
        hidden: None,
    }
}

/// Whether every word marked is in the setting's name, so its tooltip's
/// line need not show.
pub fn found_by_label(found: &Found) -> bool {
    let label = words(&found.entry.label);
    !found.words.is_empty() && found.words.iter().all(|word| label.contains(word))
}

/// `text` with each word of `marked` in it marked.
pub fn marked_text(text: &str, marked: &[String]) -> voxear_ui::RichText {
    let mut rich = voxear_ui::RichText::new();
    let mut at = 0;
    for range in marked_ranges(text, marked) {
        rich.push(&text[at..range.start], voxear_ui::Marks::default());
        rich.push(&text[range.clone()], voxear_ui::Marks { mark: true, ..Default::default() });
        at = range.end;
    }
    rich.push(&text[at..], voxear_ui::Marks::default());
    rich
}

/// The entries a search for `query` finds, best first (see the module's notes).
pub fn search(entries: &[Entry], query: &str) -> Vec<Found> {
    let typed: Vec<String> = words(query).into_iter().filter(|word| !word.is_empty()).collect();
    if typed.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(usize, Found)> =
        entries.iter().enumerate().filter_map(|(at, entry)| Some((at, rank(entry, query, &typed)?))).collect();
    found.sort_by(|(a_at, a), (b_at, b)| {
        b.score.cmp(&a.score).then(b.words.len().cmp(&a.words.len())).then(a_at.cmp(b_at))
    });
    found.into_iter().map(|(_, found)| found).collect()
}

/// How well an entry matches every word typed, or `None` where any word
/// matches nothing of it.
fn rank(entry: &Entry, query: &str, typed: &[String]) -> Option<Found> {
    let label = words(&entry.label);
    let group = words(&entry.group);
    let tip = words(&entry.tip);
    let mut score = 0;
    let mut marked: Vec<String> = Vec::new();
    for word in typed {
        let same = same_as(word);
        let best = [(&label, LABEL), (&group, GROUP), (&tip, TIP)]
            .into_iter()
            .filter_map(|(part, (own, alike))| {
                let own_hit = part.iter().find(|w| matches(w, word)).map(|w| (own, w));
                let alike_hit = || part.iter().find(|w| same.iter().any(|s| is_alike(w, s))).map(|w| (alike, w));
                own_hit.or_else(alike_hit)
            })
            .max_by_key(|(points, _)| *points)?;
        score += best.0;
        // Every word of the label and tooltip it matched, to mark.
        for part in [&label, &tip] {
            let hits = part.iter().filter(|w| matches(w, word) || same.iter().any(|s| is_alike(w, s)));
            for hit in hits {
                if !marked.contains(hit) {
                    marked.push(hit.clone());
                }
            }
        }
    }
    // A label that reads as what was typed, letter by letter, comes first.
    if let Some(close) = crate::palette::score(query, &entry.label) {
        score += 10 + close.clamp(0, 20);
    }
    if entry.hidden.is_some() {
        score -= 1;
    }
    let line = tip_line(&entry.tip, typed, &marked);
    Some(Found { entry: entry.clone(), score, words: marked, line })
}

/// The line of a tooltip with the most words typed in it, then the most
/// words found, or its first.
fn tip_line(tip: &str, typed: &[String], marked: &[String]) -> String {
    let lines: Vec<&str> = tip.split_inclusive(['.', '\n']).map(str::trim).filter(|line| !line.is_empty()).collect();
    let count = |line: &str| {
        let words = words(line);
        let own = typed.iter().filter(|typed| words.iter().any(|word| matches(word, typed))).count();
        (own, words.iter().filter(|word| marked.contains(word)).count())
    };
    let best = lines.iter().enumerate().max_by_key(|(at, line)| (count(line), std::cmp::Reverse(*at)));
    best.map_or_else(String::new, |(_, line)| line.to_string())
}

/// The words of `text`, lower case, without what is round them.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .map(|word| word.trim_matches('-').to_lowercase())
        .filter(|word| !word.is_empty())
        .collect()
}

/// Whether a word of a setting is the word typed, or starts with it, or is
/// the start of it, as `shadows` and `shadow` are.
fn matches(word: &str, typed: &str) -> bool {
    word.starts_with(typed) || (word.len() >= 4 && typed.starts_with(word) && typed.len() - word.len() <= 3)
}

/// Whether a word of a setting is `same`, a word meaning what was typed, or
/// it with a short ending, as `shadows` is: never a longer word it starts.
fn is_alike(word: &str, same: &str) -> bool {
    word.starts_with(same) && word.len() - same.len() <= 2
}

/// The words that mean much the same as `typed`.
fn same_as(typed: &str) -> Vec<&'static str> {
    let groups = SAME.iter().filter(|group| group.iter().any(|word| matches(word, typed) || matches(typed, word)));
    let mut same: Vec<&'static str> = groups.flat_map(|group| group.iter().copied()).collect();
    same.retain(|word| *word != typed);
    same.dedup();
    same
}

/// The spans of `text` a result marks: each word of `words` in it, by byte
/// range.
pub fn marked_ranges(text: &str, marked: &[String]) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        let inside = c.is_alphanumeric() || c == '-';
        match (inside, start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                if marked.contains(&text[from..at].to_lowercase()) {
                    ranges.push(from..at);
                }
                start = None;
            }
            _ => {}
        }
    }
    ranges
}

/// A row a search found, shown: scrolled to and marked for a moment.
#[derive(Default)]
pub struct Flash {
    marked: Option<(voxear_ui::NodeId, std::time::Instant)>,
}

impl Flash {
    /// How long a row found stays marked.
    const MARKED: std::time::Duration = std::time::Duration::from_millis(1500);

    /// Scrolls to `row` and marks it.
    pub fn show(&mut self, ui: &mut voxear_ui::Ui, row: voxear_ui::NodeId) {
        if let Some((old, _)) = self.marked.take() {
            ui.set_class(old, "found", false);
        }
        ui.scroll_into_view(row);
        ui.set_class(row, "found", true);
        self.marked = Some((row, std::time::Instant::now()));
    }

    /// Takes the mark off once it has shown long enough.
    pub fn update(&mut self, ui: &mut voxear_ui::Ui) {
        if let Some((row, at)) = self.marked
            && at.elapsed() > Self::MARKED
        {
            ui.set_class(row, "found", false);
            self.marked = None;
        }
    }

    /// The row marked, while one is.
    pub fn marked(&self) -> Option<voxear_ui::NodeId> {
        self.marked.map(|(row, _)| row)
    }
}

/// The row in `within` whose label says `label`, as the Inspector and the
/// settings pages lay a label out first in its row.
pub fn label_row(ui: &voxear_ui::Ui, within: voxear_ui::NodeId, label: &str) -> Option<voxear_ui::NodeId> {
    for child in ui.children(within) {
        if ui.has_class(child, "prop-label") && ui.text(child) == label && ui.is_visible(child) {
            return ui.parent(child);
        }
        if let Some(found) = label_row(ui, child, label) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, group: &str, tip: &str) -> Entry {
        Entry {
            place: Place::Preference("test"),
            label: label.into(),
            group: group.into(),
            tip: tip.into(),
            hidden: None,
        }
    }

    #[test]
    fn rough_words_and_words_that_mean_the_same_find_a_setting_by_what_it_does() {
        let entries = [
            entry("Anti-aliasing", "Lighting · Anti-Aliasing", "Smooths jagged edges. TAA also calms flicker."),
            entry("Bloom", "Lighting · Bloom", "Bright parts glow onto what is round them."),
            entry("Stick deadzone", "Project Settings · Input ▸ Buttons and Sticks", "Stops a worn stick drifting."),
        ];
        let shimmer = search(&entries, "shimmer");
        assert_eq!(shimmer.len(), 1, "{shimmer:?}");
        assert_eq!(shimmer[0].entry.label, "Anti-aliasing", "by a word that means the same");
        assert_eq!(shimmer[0].line, "TAA also calms flicker.", "the tooltip's line that matched");
        assert!(shimmer[0].words.contains(&"flicker".to_string()), "{:?}", shimmer[0].words);
        let drift = search(&entries, "stick drift");
        assert_eq!(drift[0].entry.label, "Stick deadzone");
        assert!(search(&entries, "glow edges").is_empty(), "every word must match something");
        let by_name = search(&entries, "bloom");
        assert_eq!(by_name[0].entry.label, "Bloom");
        assert!(by_name[0].score > search(&entries, "glow")[0].score, "a name typed counts more");
    }

    #[test]
    fn the_words_matched_are_marked_where_they_are() {
        let ranges = marked_ranges("TAA also calms flicker.", &["flicker".to_string()]);
        assert_eq!(ranges, [15..22]);
    }
}
