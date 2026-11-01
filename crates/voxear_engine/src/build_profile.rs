//! Build profiles: what to build, as `.buildprofile` TOML files under the
//! project's `Settings/BuildProfiles/`, after Unity 6's Build Profiles. A
//! project has as many as it likes, such as `Linux Release` and
//! `Steam Playtest`, each named by its file.
//!
//! **Easy.** A new project comes with a Release and a Development profile
//! for each platform; pick one and build it.
//!
//! **Advanced.** A profile lists only what differs from its `base`, another
//! profile by name, the way post-processing presets layer:
//!
//! ```toml
//! format = 1
//! base = "Linux Release"
//!
//! [levels]
//! start = "Levels/Menu.level"
//! include = ["Levels/Demo/*.level"]
//!
//! [development]
//! enabled = true
//! ```
//!
//! Tables merge key by key, so a profile can change one player setting and
//! keep the rest of its base's.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::bundle::Compression;

/// Where a project keeps its build profiles, and their extension.
pub const PROFILES_FOLDER: &str = "Settings/BuildProfiles";
pub const PROFILE_EXTENSION: &str = "buildprofile";
/// The platforms there are players for.
pub const PLATFORMS: [&str; 2] = ["linux-x64", "windows-x64"];

/// A build profile with its base's settings folded in, and the defaults for
/// what neither sets.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildProfile {
    pub format: u32,
    /// Another profile, by name, this one changes.
    pub base: Option<String>,
    /// `linux-x64` or `windows-x64`; the machine building, when not set.
    pub platform: Option<String>,
    pub levels: Levels,
    pub development: Development,
    pub scripting: Scripting,
    pub content: Packing,
    pub player: Player,
    pub version: Version,
    pub output: Output,
}

/// Which levels ship, and which starts.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Levels {
    /// The level the game opens on; the project's own when not set.
    pub start: Option<String>,
    /// Patterns of the levels that ship, such as `Levels/Demo/*.level`;
    /// every level when empty.
    pub include: Vec<String>,
}

/// A build to work on rather than to ship.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Development {
    /// Scripts built in Debug with their symbols, and the tools `vox run`
    /// has: the stats overlay on F1, the debug views and the fly camera.
    pub enabled: bool,
    /// Vulkan's validation layers, which find mistakes in rendering slowly.
    pub validation_layers: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scripting {
    /// Extra symbols for C#'s `#if`, such as `DEMO`.
    pub defines: Vec<String>,
}

/// How the content ships.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Packing {
    /// How bundles are compressed: `lz4`, quick to load, `zstd`, smaller,
    /// or `none`; the project's `[content]` setting when not set.
    pub compression: Option<Compression>,
}

/// How the game looks to its players: its name and window.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Player {
    /// The game's name, for its window and program; the project's when not
    /// set.
    pub product_name: Option<String>,
    pub company_name: Option<String>,
    /// `windowed`, or `fullscreen` for a borderless window that fills the
    /// screen.
    pub window: String,
    /// The window's size, when windowed.
    pub width: u32,
    pub height: u32,
}

impl Default for Player {
    fn default() -> Self {
        Self { product_name: None, company_name: None, window: "windowed".into(), width: 1600, height: 900 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Version {
    /// The game's version, as players see it, such as `1.2.0`.
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    /// Where the build goes, from the project's folder; `Builds/<profile>`
    /// when not set.
    pub folder: Option<String>,
}

impl BuildProfile {
    /// The platform built for: the profile's, or the machine's own.
    pub fn platform(&self) -> &str {
        self.platform.as_deref().unwrap_or(host_platform())
    }

    /// Where a build with this profile, named `name`, goes.
    pub fn output_dir(&self, project_root: &Path, name: &str) -> PathBuf {
        let folder = self.output.folder.clone().unwrap_or_else(|| format!("Builds/{name}"));
        project_root.join(folder)
    }
}

/// The platform this machine is, as profiles name it.
pub fn host_platform() -> &'static str {
    if cfg!(windows) { "windows-x64" } else { "linux-x64" }
}

/// Where a project's profile named `name` is.
pub fn profile_path(project_root: &Path, name: &str) -> PathBuf {
    project_root.join(PROFILES_FOLDER).join(format!("{name}.{PROFILE_EXTENSION}"))
}

/// The project's profiles, by name, in order.
pub fn profile_names(project_root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(project_root.join(PROFILES_FOLDER)) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == PROFILE_EXTENSION))
        .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

/// The profile to build when none is named: this platform's Release
/// profile, or else the first.
pub fn default_profile(project_root: &Path) -> Option<String> {
    let names = profile_names(project_root);
    let release = format!("{} Release", if cfg!(windows) { "Windows" } else { "Linux" });
    names.iter().find(|name| **name == release).or(names.first()).cloned()
}

/// Reads a profile by name, its bases folded in, the nearest winning.
pub fn load_profile(project_root: &Path, name: &str) -> Result<BuildProfile> {
    let merged = merged_table(project_root, name)?;
    let mut profile: BuildProfile =
        merged.try_into().with_context(|| format!("reading build profile `{name}` with its bases"))?;
    profile.base = None;
    if let Some(platform) = profile.platform.as_deref().filter(|p| !PLATFORMS.contains(p)) {
        bail!("build profile `{name}` is for `{platform}`; the platforms are {}", PLATFORMS.join(" and "));
    }
    if !["windowed", "fullscreen"].contains(&profile.player.window.as_str()) {
        bail!(
            "build profile `{name}` opens a `{}` window; it can be `windowed` or `fullscreen`",
            profile.player.window
        );
    }
    Ok(profile)
}

/// The profile's own base, as its file names it.
pub fn base_of(project_root: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(profile_path(project_root, name)).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    table.get("base")?.as_str().map(str::to_string)
}

/// Changes one setting of a profile in its own file, keeping its comments
/// and the rest as they are: `key` in `table` (`player`, `levels`...), or at
/// the top with `None`. A value its base already has, or `None`, takes the
/// setting out, so the profile lists only what differs from its base.
/// Makes a new profile, named "New Profile" or the next free "New Profile
/// 2", based on `base` when given, and returns its name.
pub fn add_profile(project_root: &Path, base: Option<&str>) -> Result<String> {
    let folder = project_root.join(PROFILES_FOLDER);
    std::fs::create_dir_all(&folder).with_context(|| format!("making {}", folder.display()))?;
    let names = profile_names(project_root);
    let name = (1..)
        .map(|n| if n == 1 { "New Profile".to_string() } else { format!("New Profile {n}") })
        .find(|name| !names.contains(name))
        .context("no free name")?;
    let text = match base {
        Some(base) => format!("format = 1\nbase = {}\n", toml::Value::from(base)),
        None => "format = 1\n".to_string(),
    };
    let path = profile_path(project_root, &name);
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(name)
}

/// Renames a profile, and every profile based on it follows the new name.
pub fn rename_profile(project_root: &Path, from: &str, to: &str) -> Result<()> {
    let to = to.trim();
    ensure!(!to.is_empty() && !to.contains(['/', '\\']), "a profile's name cannot be empty or hold a slash");
    if from == to {
        return Ok(());
    }
    ensure!(!profile_names(project_root).iter().any(|name| name == to), "there is already a profile called {to}");
    // Those based on it first, while it is still there to read their settings through.
    for name in profile_names(project_root) {
        if base_of(project_root, &name).as_deref() == Some(from) {
            set_setting(project_root, &name, None, "base", Some(to.into()))?;
        }
    }
    std::fs::rename(profile_path(project_root, from), profile_path(project_root, to))
        .with_context(|| format!("renaming {from} to {to}"))
}

/// Takes a profile out, moving its file into the project's
/// `.voxear/Trash` rather than destroying it. A profile others are based on
/// is kept, as they need it.
pub fn remove_profile(project_root: &Path, name: &str) -> Result<()> {
    let based: Vec<String> =
        profile_names(project_root).into_iter().filter(|n| base_of(project_root, n).as_deref() == Some(name)).collect();
    ensure!(based.is_empty(), "{} is based on {name}, so {name} stays", based.join(" and "));
    let trash = project_root.join(".voxear").join("Trash").join(PROFILES_FOLDER);
    std::fs::create_dir_all(&trash).with_context(|| format!("making {}", trash.display()))?;
    let file = format!("{name}.{PROFILE_EXTENSION}");
    std::fs::rename(profile_path(project_root, name), trash.join(file)).with_context(|| format!("taking {name} out"))
}

pub fn set_setting(
    project_root: &Path,
    name: &str,
    table: Option<&str>,
    key: &str,
    value: Option<toml::Value>,
) -> Result<()> {
    let path = profile_path(project_root, name);
    let text = std::fs::read_to_string(&path).with_context(|| format!("no build profile `{name}`"))?;
    let mut document: toml_edit::DocumentMut = text.parse().with_context(|| format!("reading {}", path.display()))?;
    let inherited = match base_of(project_root, name) {
        Some(base) => {
            let merged = merged_table(project_root, &base)?;
            let within = match table {
                Some(table) => merged.get(table).and_then(|t| t.as_table()).and_then(|t| t.get(key)).cloned(),
                None => merged.get(key).cloned(),
            };
            within.or_else(|| default_value(table, key))
        }
        None => default_value(table, key),
    };
    let value = value.filter(|value| Some(value) != inherited.as_ref());
    match (table, value) {
        (Some(table), Some(value)) => {
            let edited: toml_edit::Value = value.to_string().parse().context("writing the setting")?;
            let section = document.entry(table).or_insert_with(toml_edit::table);
            let section = section.as_table_mut().with_context(|| format!("[{table}] in {name} is not a table"))?;
            section[key] = toml_edit::value(edited);
        }
        (Some(table), None) => {
            if let Some(section) = document.get_mut(table).and_then(|t| t.as_table_mut()) {
                section.remove(key);
                if section.is_empty() {
                    document.remove(table);
                }
            }
        }
        (None, Some(value)) => {
            let edited: toml_edit::Value = value.to_string().parse().context("writing the setting")?;
            document[key] = toml_edit::value(edited);
        }
        (None, None) => drop(document.remove(key)),
    }
    std::fs::write(&path, document.to_string()).with_context(|| format!("writing {}", path.display()))
}

/// What a profile has for a setting when nothing sets it.
fn default_value(table: Option<&str>, key: &str) -> Option<toml::Value> {
    let defaults = toml::Table::try_from(BuildProfile::default()).ok()?;
    match table {
        Some(table) => defaults.get(table)?.as_table()?.get(key).cloned(),
        None => defaults.get(key).cloned(),
    }
}

/// A profile's settings as a table, its bases' folded in, the nearest
/// winning.
fn merged_table(project_root: &Path, name: &str) -> Result<toml::Table> {
    let mut chain = Vec::new();
    let mut next = Some(name.to_string());
    while let Some(current) = next {
        if chain.iter().any(|(seen, _)| *seen == current) {
            bail!("build profile `{name}` has a loop of bases through `{current}`");
        }
        let path = profile_path(project_root, &current);
        let text = std::fs::read_to_string(&path).with_context(|| format!("no build profile `{current}`"))?;
        let table: toml::Table = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        next = table.get("base").and_then(|base| base.as_str()).map(str::to_string);
        chain.push((current, table));
    }
    let mut merged = toml::Table::new();
    for (_, table) in chain.into_iter().rev() {
        merge(&mut merged, table);
    }
    // The profile asked for is the one named, whatever its bases say.
    merged.insert("base".into(), toml::Value::String(name.to_string()));
    Ok(merged)
}

/// Folds `over` into `into`, table by table, its values winning.
fn merge(into: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (into.get_mut(&key), value) {
            (Some(toml::Value::Table(inner)), toml::Value::Table(value)) => merge(inner, value),
            (_, value) => {
                into.insert(key, value);
            }
        }
    }
}

/// The profiles a new project starts with: a Release and a Development
/// profile for each platform, Development changing Release. File names and
/// what each holds.
pub fn default_profiles() -> Vec<(String, String)> {
    let mut profiles = Vec::new();
    for (platform, label) in [("linux-x64", "Linux"), ("windows-x64", "Windows")] {
        let release = format!(
            "# What a {label} release build ships. Profiles that change it name it as their base.\n\
             format = 1\nplatform = \"{platform}\"\n\n[player]\nwindow = \"windowed\"\nwidth = 1600\nheight = 900\n"
        );
        let development = format!(
            "# A {label} build to work on: scripts in Debug, and the stats overlay on F1.\n\
             format = 1\nbase = \"{label} Release\"\n\n[development]\nenabled = true\n"
        );
        profiles.push((format!("{label} Release.{PROFILE_EXTENSION}"), release));
        profiles.push((format!("{label} Development.{PROFILE_EXTENSION}"), development));
    }
    profiles
}

/// Writes the default profiles into a project that has none.
pub fn write_default_profiles(project_root: &Path) -> Result<()> {
    let folder = project_root.join(PROFILES_FOLDER);
    if !profile_names(project_root).is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(&folder).with_context(|| format!("making {}", folder.display()))?;
    for (file, text) in default_profiles() {
        std::fs::write(folder.join(&file), text).with_context(|| format!("writing {file}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_are_added_renamed_with_those_based_on_them_and_taken_out_to_the_trash() {
        let dir = tempfile::tempdir().unwrap();
        write_default_profiles(dir.path()).unwrap();
        assert_eq!(add_profile(dir.path(), Some("Linux Release")).unwrap(), "New Profile");
        assert_eq!(base_of(dir.path(), "New Profile").as_deref(), Some("Linux Release"));
        assert_eq!(add_profile(dir.path(), None).unwrap(), "New Profile 2", "the next free name");
        set_setting(dir.path(), "New Profile 2", None, "base", Some("New Profile".into())).unwrap();
        rename_profile(dir.path(), "New Profile", "Demo").unwrap();
        assert_eq!(base_of(dir.path(), "New Profile 2").as_deref(), Some("Demo"), "what is based on it follows");
        assert!(rename_profile(dir.path(), "Demo", "Linux Release").is_err(), "a name taken");
        assert!(remove_profile(dir.path(), "Demo").is_err(), "New Profile 2 needs it");
        remove_profile(dir.path(), "New Profile 2").unwrap();
        assert!(!profile_names(dir.path()).contains(&"New Profile 2".to_string()));
        assert!(dir.path().join(".voxear/Trash/Settings/BuildProfiles/New Profile 2.buildprofile").is_file());
    }

    #[test]
    fn a_profile_changes_its_base_key_by_key() {
        let dir = tempfile::tempdir().unwrap();
        write_default_profiles(dir.path()).unwrap();
        assert_eq!(
            profile_names(dir.path()),
            ["Linux Development", "Linux Release", "Windows Development", "Windows Release"]
        );
        let release = load_profile(dir.path(), "Linux Release").unwrap();
        assert!(!release.development.enabled);
        assert_eq!((release.platform(), release.player.width), ("linux-x64", 1600));

        let development = load_profile(dir.path(), "Linux Development").unwrap();
        assert!(development.development.enabled, "its own setting");
        assert_eq!((development.platform(), development.player.height), ("linux-x64", 900), "and its base's");

        let demo = "format = 1\nbase = \"Linux Development\"\n[player]\nwidth = 1280\nproduct_name = \"Demo\"\n";
        std::fs::write(profile_path(dir.path(), "Demo"), demo).unwrap();
        let demo = load_profile(dir.path(), "Demo").unwrap();
        assert_eq!((demo.player.width, demo.player.height), (1280, 900), "one player setting changed, the rest kept");
        assert_eq!(demo.player.product_name.as_deref(), Some("Demo"));
        assert!(demo.development.enabled, "through two bases");
        assert_eq!(demo.output_dir(dir.path(), "Demo"), dir.path().join("Builds/Demo"));
    }

    #[test]
    fn a_setting_is_written_where_it_differs_from_the_base_keeping_comments() {
        let dir = tempfile::tempdir().unwrap();
        write_default_profiles(dir.path()).unwrap();
        let path = profile_path(dir.path(), "Linux Development");
        set_setting(dir.path(), "Linux Development", Some("player"), "width", Some(1280.into())).unwrap();
        set_setting(dir.path(), "Linux Development", Some("scripting"), "defines", Some(vec!["DEMO"].into())).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# A Linux build to work on"), "its comment is kept: {text}");
        let profile = load_profile(dir.path(), "Linux Development").unwrap();
        assert_eq!((profile.player.width, profile.player.height), (1280, 900));
        assert_eq!(profile.scripting.defines, ["DEMO"]);

        // The base's own value, or none, takes the setting out again.
        set_setting(dir.path(), "Linux Development", Some("player"), "width", Some(1600.into())).unwrap();
        set_setting(dir.path(), "Linux Development", Some("scripting"), "defines", None).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("[player]") && !text.contains("defines"), "only what differs: {text}");
        set_setting(dir.path(), "Linux Release", None, "platform", Some("windows-x64".into())).unwrap();
        assert_eq!(
            load_profile(dir.path(), "Linux Development").unwrap().platform(),
            "windows-x64",
            "through the base"
        );
    }

    #[test]
    fn loops_and_unknown_platforms_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(PROFILES_FOLDER)).unwrap();
        std::fs::write(profile_path(dir.path(), "A"), "base = \"B\"\n").unwrap();
        std::fs::write(profile_path(dir.path(), "B"), "base = \"A\"\n").unwrap();
        assert!(load_profile(dir.path(), "A").unwrap_err().to_string().contains("loop"));
        std::fs::write(profile_path(dir.path(), "Mac"), "platform = \"macos-arm64\"\n").unwrap();
        assert!(load_profile(dir.path(), "Mac").is_err());
        std::fs::write(profile_path(dir.path(), "Big"), "[player]\nwindow = \"huge\"\n").unwrap();
        assert!(load_profile(dir.path(), "Big").is_err());
        assert!(load_profile(dir.path(), "Missing").is_err());
    }
}
