//! A level's own scenes, as the Inspector shows them for a `.level`
//! selected in the Project, and the changes it makes to them: a scene added
//! at the end, one put in another's place, one taken out, another made the
//! active one, or one no longer required. Each change is written to the file at once, keeping its
//! comments and the rest as they were, as a material's fields are. A scene
//! added is written by its GUID, so renaming it keeps the level working, with
//! its path in a comment beside it, as the samples write them.

use anyhow::{Context, Result, ensure};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

/// One of a level's own scenes, as its file lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelEntry {
    /// The scene's GUID or path, as written.
    pub scene: String,
    /// Where new Objs go: the one marked so, or the first.
    pub active: bool,
    /// Off, the level passes over it if it fails to load.
    pub required: bool,
}

/// A change to a level's scenes, by their place in its file.
#[derive(Clone, Debug, PartialEq)]
pub enum LevelEdit {
    /// A scene added at the end: its GUID, and its path for the comment.
    Add {
        guid: String,
        path: String,
    },
    /// Another scene in this one's place, keeping its flags.
    Replace {
        at: usize,
        guid: String,
        path: String,
    },
    Remove(usize),
    /// This scene is the active one, and no other is marked so.
    Activate(usize),
    Require(usize, bool),
}

/// A level file's own scenes, in order, not those of the level it stands on.
/// The active one is the last marked so, as loading reads it, or the first.
pub fn entries(text: &str) -> Result<Vec<LevelEntry>> {
    let document: DocumentMut = text.parse().context("reading the level")?;
    let flag = |table: &Table, key: &str| table.get(key).and_then(Item::as_bool);
    let mut entries: Vec<LevelEntry> = scenes(&document)
        .map(|table| LevelEntry {
            scene: table.get("scene").and_then(Item::as_str).unwrap_or_default().to_string(),
            active: flag(table, "active").unwrap_or(false),
            required: flag(table, "required").unwrap_or(true),
        })
        .collect();
    let active = entries.iter().rposition(|entry| entry.active).unwrap_or(0);
    for (at, entry) in entries.iter_mut().enumerate() {
        entry.active = at == active;
    }
    Ok(entries)
}

/// The level's text with `edit` made.
pub fn edit(text: &str, edit: &LevelEdit) -> Result<String> {
    let mut document: DocumentMut = text.parse().context("reading the level")?;
    let count = scenes(&document).count();
    let index = match edit {
        LevelEdit::Add { .. } => None,
        LevelEdit::Replace { at, .. } | LevelEdit::Remove(at) | LevelEdit::Activate(at) | LevelEdit::Require(at, _) => {
            Some(*at)
        }
    };
    ensure!(index.is_none_or(|at| at < count), "the level has no scene {}", index.unwrap_or_default() + 1);
    let list = document.entry("scene").or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
    let list = list.as_array_of_tables_mut().context("the level's `scene` is not a list of [[scene]] tables")?;
    match edit {
        LevelEdit::Add { guid, path } => {
            let mut table = Table::new();
            set_scene(&mut table, guid, path);
            // A line between scenes, as a person writes them.
            table.decor_mut().set_prefix("\n");
            list.push(table);
        }
        LevelEdit::Replace { at, guid, path } => {
            set_scene(list.get_mut(*at).context("the scene is listed")?, guid, path);
        }
        LevelEdit::Remove(at) => {
            list.remove(*at);
        }
        LevelEdit::Activate(at) => {
            for (place, table) in list.iter_mut().enumerate() {
                if place == *at {
                    table["active"] = value(true);
                } else {
                    table.remove("active");
                }
            }
        }
        LevelEdit::Require(at, required) => {
            let table = list.get_mut(*at).context("the scene is listed")?;
            if *required {
                table.remove("required");
            } else {
                table["required"] = value(false);
            }
        }
    }
    if list.is_empty() {
        document.remove("scene");
    }
    Ok(document.to_string())
}

/// Points a level's entry at a scene by its GUID, with its path in a
/// comment beside it.
fn set_scene(table: &mut Table, guid: &str, path: &str) {
    table["scene"] = value(guid);
    if let Some(scene) = table["scene"].as_value_mut() {
        scene.decor_mut().set_suffix(format!("  # {path}"));
    }
}

fn scenes(document: &DocumentMut) -> impl Iterator<Item = &Table> {
    document.get("scene").and_then(Item::as_array_of_tables).into_iter().flat_map(ArrayOfTables::iter)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: &str = "# Voxear level\nformat = 1\nstands_on = \"Levels/World.level\"\n\n[[scene]]\n\
                       scene = \"713a4d989c654c6ca23b1d63201e0090\"  # Scenes/Day.scene\n";

    #[test]
    fn scenes_are_added_marked_and_taken_out_keeping_the_comments() {
        let added = edit(DAY, &LevelEdit::Add { guid: "abc".into(), path: "Scenes/Lights.scene".into() }).unwrap();
        assert!(added.starts_with("# Voxear level\n"), "{added}");
        assert!(added.contains("scene = \"713a4d989c654c6ca23b1d63201e0090\"  # Scenes/Day.scene\n"), "{added}");
        assert!(added.ends_with("\n[[scene]]\nscene = \"abc\"  # Scenes/Lights.scene\n"), "{added}");
        let listed = entries(&added).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].active && !listed[1].active, "the first is active until another is marked");

        let marked = edit(&added, &LevelEdit::Activate(1)).unwrap();
        let marked = edit(&marked, &LevelEdit::Require(0, false)).unwrap();
        let listed = entries(&marked).unwrap();
        assert!(!listed[0].active && listed[1].active);
        assert!(!listed[0].required && listed[1].required);
        assert!(marked.contains("required = false") && marked.contains("active = true"), "{marked}");
        let marked = edit(&marked, &LevelEdit::Require(0, true)).unwrap();
        assert!(!marked.contains("required"), "required is the default, so it is not written:\n{marked}");

        let replaced =
            edit(&marked, &LevelEdit::Replace { at: 1, guid: "def".into(), path: "Scenes/Dusk.scene".into() });
        let replaced = replaced.unwrap();
        assert!(replaced.contains("scene = \"def\"  # Scenes/Dusk.scene\nactive = true"), "{replaced}");
        assert!(!replaced.contains("abc"), "{replaced}");

        let taken = edit(&marked, &LevelEdit::Remove(1)).unwrap();
        assert_eq!(
            entries(&taken).unwrap(),
            [LevelEntry { scene: listed[0].scene.clone(), active: true, required: true }]
        );
        let empty = edit(&taken, &LevelEdit::Remove(0)).unwrap();
        assert!(!empty.contains("[[scene]]") && empty.contains("stands_on"), "{empty}");
        assert!(edit(&empty, &LevelEdit::Remove(0)).is_err(), "no scene to take out");
        let again = edit(&empty, &LevelEdit::Add { guid: "abc".into(), path: "Scenes/Lights.scene".into() }).unwrap();
        assert_eq!(entries(&again).unwrap().len(), 1, "a level made empty takes scenes again:\n{again}");
    }

    #[test]
    fn a_new_level_takes_its_first_scene() {
        let new = "# Voxear level: list its scenes as [[scene]] tables.\nformat = 1\n";
        let text = edit(new, &LevelEdit::Add { guid: "abc".into(), path: "Scenes/Castle.scene".into() }).unwrap();
        let file: voxear_engine::LevelFile = toml::from_str(&text).unwrap();
        assert_eq!(file.scenes.len(), 1, "{text}");
        assert_eq!(file.scenes[0].scene, "abc");
    }
}
