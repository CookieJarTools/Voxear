//! `vox upgrade`: moves a project onto this engine's version, as the Voxear
//! Launcher asks when a project is opened with a newer Voxear (or an older
//! one, which the launcher asks the person about first). The launcher has
//! backed the project up before it runs this, and checks the pin after.
//!
//! What it does, each part said as a change in plain words:
//! - checks that every level, scene and prefab is in a format this version
//!   reads; one saved by a newer Voxear stops it before anything changes
//! - pins `voxear.toml` to this version, keeping the rest of the file as
//!   it is
//! - writes `.voxear/Engine.props`, so the scripts build against this engine
//! - adds what projects of this version have and older ones may lack: the
//!   default build profiles
//! - gives assets without one a `.meta` file
//!
//! Later versions add their own conversions here, each saying what it
//! changed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::{
    LEVEL_FORMAT, PROJECT_FILE, Project, build_profile, engine_version, prefab::PREFAB_FORMAT, scene_file::SCENE_FORMAT,
};

/// The format of what `vox upgrade --json` prints, as `engine.toml`'s
/// `[supports]` names it for the launcher.
pub const FORMAT: u32 = 1;

/// What an upgrade tells whoever asked for it, as it goes.
#[derive(Debug, PartialEq)]
pub enum Event {
    /// What it is doing now.
    Step(String),
    /// What it changed, for the person to read.
    Change(String),
}

/// Folders an upgrade does not look in: the engine's caches, builds, the
/// project's documentation, and version control and C# build output.
const SKIPPED: [&str; 5] = ["Builds", "Docs", "bin", "obj", "target"];

/// Moves the project in `folder` onto this engine's version. `from` is the
/// version it was pinned to, if it was.
pub fn upgrade(folder: &Path, from: Option<&str>, report: &mut dyn FnMut(Event)) -> Result<()> {
    let version = engine_version();
    report(Event::Step("Reading voxear.toml".into()));
    let project = Project::open(folder)?;
    let root = project.root.clone();

    report(Event::Step("Checking the levels, scenes and prefabs".into()));
    for (path, extension) in saved_files(&root) {
        let known = match extension.as_str() {
            "level" => LEVEL_FORMAT,
            "scene" => SCENE_FORMAT,
            _ => PREFAB_FORMAT,
        };
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let table: toml::Table = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        let format = table.get("format").and_then(|f| f.as_integer()).unwrap_or(1);
        if format > i64::from(known) {
            let shown = path.strip_prefix(&root).unwrap_or(&path).display().to_string();
            bail!("{shown} was saved by a newer Voxear (format {format}), which Voxear {version} cannot read");
        }
    }

    report(Event::Step("Pinning the project to this version".into()));
    let manifest = root.join(PROJECT_FILE);
    let text = std::fs::read_to_string(&manifest).with_context(|| format!("reading {}", manifest.display()))?;
    let mut document: toml_edit::DocumentMut =
        text.parse().with_context(|| format!("reading {}", manifest.display()))?;
    let section = document["project"].as_table_mut().context("voxear.toml has no [project]")?;
    section["engine"] = toml_edit::value(version);
    std::fs::write(&manifest, document.to_string()).with_context(|| format!("writing {}", manifest.display()))?;
    report(Event::Change(match from {
        Some(from) if from != version => format!("Moved from Voxear {from} to Voxear {version}"),
        Some(_) => format!("Already on Voxear {version}; checked it over"),
        None => format!("Pinned to Voxear {version}, as it was not pinned before"),
    }));

    if project.scripts_project().is_some() {
        report(Event::Step("Pointing the scripts at this engine".into()));
        let props = root.join(".voxear").join("Engine.props");
        std::fs::create_dir_all(root.join(".voxear"))?;
        std::fs::write(&props, crate::scripts::engine_props())
            .with_context(|| format!("writing {}", props.display()))?;
        let modules = root.join(".voxear").join("Modules.props");
        std::fs::write(&modules, crate::scripts::modules_props(&project.modules()))
            .with_context(|| format!("writing {}", modules.display()))?;
        report(Event::Change(format!("The scripts now build against Voxear {version}")));
    }

    report(Event::Step("Adding what this version's projects have".into()));
    if build_profile::profile_names(&root).is_empty() {
        build_profile::write_default_profiles(&root)?;
        report(Event::Change(format!(
            "Added a Release and a Development build profile for Linux and Windows, in {}",
            build_profile::PROFILES_FOLDER
        )));
    }

    report(Event::Step("Giving the assets their IDs".into()));
    let (_, scan) = voxear_assets::AssetDb::scan(&root).context("writing the assets' .meta files")?;
    match scan.created.len() {
        0 => {}
        1 => report(Event::Change(format!("Gave {} its .meta file", scan.created[0]))),
        count => report(Event::Change(format!("Gave {count} assets their .meta files"))),
    }
    Ok(())
}

/// Every level, scene and prefab in the project, with its extension.
fn saved_files(folder: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            found.extend(saved_files(&path));
            continue;
        }
        let extension = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        if ["level", "scene", "prefab"].contains(&extension.as_str()) {
            found.push((path, extension));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(pin: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let manifest = format!("# Orbit's settings\n[project]\nname = \"Orbit\"\n{pin}start_level = \"Main.level\"\n");
        std::fs::write(dir.path().join(PROJECT_FILE), manifest).unwrap();
        std::fs::write(dir.path().join("Main.level"), "format = 1\nscenes = []\n").unwrap();
        dir
    }

    #[test]
    fn a_project_moves_onto_this_version_saying_what_changed() {
        let dir = project("engine = \"0.0.9\"\n");
        let mut events = Vec::new();
        upgrade(dir.path(), Some("0.0.9"), &mut |event| events.push(event)).unwrap();
        let changes: Vec<&String> = events
            .iter()
            .filter_map(|e| match e {
                Event::Change(change) => Some(change),
                Event::Step(_) => None,
            })
            .collect();
        assert_eq!(changes[0], &format!("Moved from Voxear 0.0.9 to Voxear {}", engine_version()));
        assert!(changes[1].starts_with("Added a Release and a Development build profile"), "{changes:?}");
        assert_eq!(changes[2], "Gave Main.level its .meta file");
        let manifest = std::fs::read_to_string(dir.path().join(PROJECT_FILE)).unwrap();
        assert!(manifest.starts_with("# Orbit's settings\n"), "the rest kept as it was: {manifest}");
        assert_eq!(Project::open(dir.path()).unwrap().file.project.engine.as_deref(), Some(engine_version()));

        // Again, it has nothing left to add.
        let mut again = Vec::new();
        upgrade(dir.path(), Some(engine_version()), &mut |event| again.push(event)).unwrap();
        assert_eq!(again.iter().filter(|e| matches!(e, Event::Change(_))).count(), 1, "{again:?}");
    }

    #[test]
    fn a_file_from_a_newer_voxear_stops_it_before_anything_changes() {
        let dir = project("");
        std::fs::write(dir.path().join("Main.level"), "format = 99\nscenes = []\n").unwrap();
        let error = upgrade(dir.path(), None, &mut |_| {}).unwrap_err().to_string();
        assert!(error.contains("Main.level was saved by a newer Voxear"), "{error}");
        let manifest = std::fs::read_to_string(dir.path().join(PROJECT_FILE)).unwrap();
        assert!(!manifest.contains("engine"), "not pinned: {manifest}");
    }
}
