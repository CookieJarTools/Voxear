//! What the Project panel does to the project's files. New folders, scenes,
//! levels, materials and C# scripts get names not yet taken ("New Scene 1").
//! Renaming takes an asset's `.meta` along, so its GUID and every reference
//! to it hold, and renaming a script renames the class it declares, which
//! must match the file. Duplicating copies the file without its `.meta`, so
//! the copy gets a GUID of its own. Deleting moves a file and its `.meta`
//! into the project's `.voxear/Trash`, where it can be got back, rather than
//! destroying it. Files dropped on the panel from outside the editor are
//! copied in, numbered when their names are taken.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

/// Something the Project panel makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewFile {
    Folder,
    Scene,
    Level,
    Material,
    Script,
}

impl NewFile {
    pub const ALL: [NewFile; 5] = [NewFile::Folder, NewFile::Scene, NewFile::Level, NewFile::Material, NewFile::Script];

    /// As the menu offers it.
    pub fn label(self) -> &'static str {
        match self {
            NewFile::Folder => "New Folder",
            NewFile::Scene => "New Scene",
            NewFile::Level => "New Level",
            NewFile::Material => "New Material",
            NewFile::Script => "New C# Script",
        }
    }

    /// The name it is made with, before a number makes it unique, and its
    /// extension.
    fn name(self) -> (&'static str, &'static str) {
        match self {
            NewFile::Folder => ("New Folder", ""),
            NewFile::Scene => ("New Scene", ".scene"),
            NewFile::Level => ("New Level", ".level"),
            NewFile::Material => ("New Material", ".material"),
            NewFile::Script => ("NewBehaviour", ".cs"),
        }
    }
}

/// Makes `kind` in `folder` (from the project's folder) and returns its path
/// from the project's folder. A script's class is in `namespace`.
pub fn create(root: &Path, folder: &str, kind: NewFile, namespace: &str) -> Result<String> {
    let (stem, extension) = kind.name();
    let name = unique(&root.join(folder), stem, extension);
    let path = join(folder, &name);
    let file = root.join(&path);
    match kind {
        NewFile::Folder => std::fs::create_dir_all(&file),
        NewFile::Scene => std::fs::write(&file, "# Voxear scene\nformat = 1\n"),
        NewFile::Level => std::fs::write(&file, "# Voxear level: list its scenes as [[scene]] tables.\nformat = 1\n"),
        NewFile::Material => std::fs::write(&file, "color = [0.8, 0.8, 0.8]\nmetallic = 0.0\nroughness = 0.5\n"),
        NewFile::Script => std::fs::write(&file, script(namespace, &name[..name.len() - extension.len()])),
    }
    .with_context(|| format!("making {}", file.display()))?;
    Ok(path)
}

/// Renames a file or folder in place, with its `.meta`, and returns its new
/// path. A file keeps its extension, and a script's class follows its name.
pub fn rename(root: &Path, path: &str, name: &str) -> Result<String> {
    let name = name.trim();
    ensure!(!name.is_empty() && !name.contains(['/', '\\']), "a name cannot be empty or have slashes in it");
    let from = root.join(path);
    // A file keeps its kind, as in Unity's Project window: a name typed
    // without its extension gets it back.
    let extension = split_extension(path.rsplit('/').next().unwrap_or(path)).1;
    let name = match from.is_file() && !extension.is_empty() && !name.ends_with(extension) {
        true => format!("{name}{extension}"),
        false => name.to_string(),
    };
    let name = name.as_str();
    let folder = parent(path);
    let renamed = join(folder, name);
    let to = root.join(&renamed);
    if renamed == path {
        return Ok(renamed);
    }
    ensure!(!to.exists(), "{name} is already there");
    std::fs::rename(&from, &to).with_context(|| format!("renaming {path}"))?;
    let meta = meta_of(&from);
    if meta.exists() {
        std::fs::rename(&meta, meta_of(&to)).with_context(|| format!("renaming {path}'s .meta"))?;
    }
    if path.ends_with(".cs") && name.ends_with(".cs") {
        rename_class(&to, stem(path), stem(name))?;
    }
    Ok(renamed)
}

/// Copies a file beside itself, without its `.meta`, and returns the copy's
/// path. Folders are not copied.
pub fn duplicate(root: &Path, path: &str) -> Result<String> {
    let from = root.join(path);
    ensure!(from.is_file(), "only files can be duplicated");
    let name = path.rsplit('/').next().unwrap_or(path);
    let (base, extension) = split_extension(name);
    let copy = join(parent(path), &unique(&root.join(parent(path)), base, extension));
    std::fs::copy(&from, root.join(&copy)).with_context(|| format!("copying {path}"))?;
    if path.ends_with(".cs") {
        rename_class(&root.join(&copy), base, stem(&copy))?;
    }
    Ok(copy)
}

/// Copies files and folders from outside the project into `folder` (from
/// the project's folder), as dropped on the Project panel, and returns their
/// project paths. A name already taken is numbered, and a script then
/// renamed to match. A `.meta` dropped with its file follows it, keeping its
/// GUID and import settings; any other `.meta` is left out. A file already
/// in `folder` stays as it is.
pub fn bring_in(root: &Path, folder: &str, files: &[PathBuf]) -> Result<Vec<String>> {
    let into = root.join(folder);
    let is_meta = |file: &&PathBuf| file.extension().is_some_and(|e| e == "meta");
    let (metas, files): (Vec<&PathBuf>, Vec<&PathBuf>) = files.iter().partition(is_meta);
    let mut brought = Vec::new();
    // Each file's name, and the name it has in the project.
    let mut named = Vec::new();
    for file in files {
        let name = file.file_name().context("a file has no name")?.to_string_lossy().into_owned();
        if file.parent() == Some(into.as_path()) {
            brought.push(join(folder, &name));
            continue;
        }
        let (base, extension) = if file.is_dir() { (name.as_str(), "") } else { split_extension(&name) };
        let free = unique(&into, base, extension);
        copy_all(file, &into.join(&free)).with_context(|| format!("copying {}", file.display()))?;
        if free != name && name.ends_with(".cs") {
            rename_class(&into.join(&free), base, stem(&free))?;
        }
        brought.push(join(folder, &free));
        named.push((name, free));
    }
    for meta in metas {
        let name = meta.file_name().unwrap_or_default().to_string_lossy();
        let of = name.trim_end_matches(".meta");
        if let Some((_, free)) = named.iter().find(|(name, _)| name == of) {
            std::fs::copy(meta, into.join(format!("{free}.meta"))).with_context(|| format!("copying {name}"))?;
        }
    }
    Ok(brought)
}

/// Copies a file, or a folder with everything in it.
fn copy_all(from: &Path, to: &Path) -> Result<()> {
    if !from.is_dir() {
        std::fs::copy(from, to)?;
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy_all(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// Moves a file or folder, and its `.meta`, into `.voxear/Trash`, and
/// returns where it went.
pub fn trash(root: &Path, path: &str) -> Result<PathBuf> {
    let from = root.join(path);
    ensure!(from.exists(), "{path} is not there");
    let bin = root.join(".voxear").join("Trash");
    std::fs::create_dir_all(&bin)?;
    let name = path.rsplit('/').next().unwrap_or(path);
    let to = bin.join(unique(&bin, name, ""));
    std::fs::rename(&from, &to).with_context(|| format!("moving {path} to the trash"))?;
    let meta = meta_of(&from);
    if meta.exists() {
        std::fs::rename(&meta, meta_of(&to))?;
    }
    Ok(to)
}

/// A path for a new file in `folder` (from the project's folder), named
/// `stem` with `extension`, numbered if that is taken.
pub fn free_path(root: &Path, folder: &str, stem: &str, extension: &str) -> String {
    join(folder, &unique(&root.join(folder), stem, extension))
}

/// `stem` and `extension`, with a number before the extension when the name
/// is taken: "New Scene.scene", then "New Scene 1.scene".
fn unique(folder: &Path, stem: &str, extension: &str) -> String {
    let free = |name: &str| !folder.join(name).exists();
    let first = format!("{stem}{extension}");
    if free(&first) {
        return first;
    }
    let spaced = !stem.ends_with(|c: char| c.is_alphanumeric()) || !extension.eq(".cs");
    (1..)
        .map(|n| if spaced { format!("{stem} {n}{extension}") } else { format!("{stem}{n}{extension}") })
        .find(|name| free(name))
        .expect("some number is free")
}

/// A new behaviour, named for its file, in today's C#: a namespace for the
/// whole file, on one line.
fn script(namespace: &str, class: &str) -> String {
    format!(
        "using Voxear;\n\nnamespace {namespace};\n\npublic sealed class {class} : Behaviour\n{{\n\tprotected override void \
         Start()\n\t{{\n\t}}\n\n\tprotected override void Update()\n\t{{\n\t}}\n}}\n"
    )
}

/// Renames the class a script declares, which must match its file.
fn rename_class(file: &Path, from: &str, to: &str) -> Result<()> {
    let valid = to.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && to.chars().all(|c| c.is_alphanumeric() || c == '_');
    if !valid || from == to {
        return Ok(());
    }
    let text = std::fs::read_to_string(file)?;
    let declared = format!("class {from}");
    if !text.contains(&declared) {
        return Ok(());
    }
    std::fs::write(file, text.replacen(&declared, &format!("class {to}"), 1))?;
    Ok(())
}

fn meta_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".meta");
    path.with_file_name(name)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

/// A file's name before its extension, and its extension with the dot.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) => (&name[..dot], &name[dot..]),
        None => (name, ""),
    }
}

fn stem(name: &str) -> &str {
    let name = name.rsplit('/').next().unwrap_or(name);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_made_renamed_copied_and_trashed_with_their_meta() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Scenes")).unwrap();
        assert_eq!(create(root, "Scenes", NewFile::Scene, "Game").unwrap(), "Scenes/New Scene.scene");
        assert_eq!(create(root, "Scenes", NewFile::Scene, "Game").unwrap(), "Scenes/New Scene 1.scene", "a free name");
        std::fs::write(root.join("Scenes/New Scene.scene.meta"), "guid = \"x\"\n").unwrap();

        let renamed = rename(root, "Scenes/New Scene.scene", "Main").unwrap();
        assert_eq!(renamed, "Scenes/Main.scene", "a file keeps its extension when the name leaves it out");
        assert_eq!(rename(root, "Scenes/Main.scene", "Main.scene").unwrap(), "Scenes/Main.scene", "or has it");
        assert!(root.join("Scenes/Main.scene.meta").is_file(), "the .meta goes along, keeping the GUID");

        let script = create(root, "", NewFile::Script, "Game").unwrap();
        assert_eq!(script, "NewBehaviour.cs");
        rename(root, &script, "Mover.cs").unwrap();
        assert!(std::fs::read_to_string(root.join("Mover.cs")).unwrap().contains("class Mover : Behaviour"));
        let copy = duplicate(root, "Mover.cs").unwrap();
        assert_eq!(copy, "Mover1.cs");
        assert!(std::fs::read_to_string(root.join(&copy)).unwrap().contains("class Mover1"));

        let gone = trash(root, "Scenes/Main.scene").unwrap();
        assert!(!root.join("Scenes/Main.scene").exists() && gone.is_file() && meta_of(&gone).is_file());
    }

    #[test]
    fn files_dropped_from_outside_are_copied_in_with_their_meta() {
        let (outside, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (from, root) = (outside.path(), project.path());
        std::fs::create_dir_all(root.join("Models")).unwrap();
        std::fs::write(root.join("Models/Ship.glb"), "old").unwrap();
        for (name, text) in [("Ship.glb", "new"), ("Ship.glb.meta", "guid = \"x\"\n"), ("Lone.png.meta", "")] {
            std::fs::write(from.join(name), text).unwrap();
        }
        std::fs::create_dir_all(from.join("Rocks/Small")).unwrap();
        std::fs::write(from.join("Rocks/Small/Pebble.glb"), "").unwrap();
        let dropped = ["Ship.glb.meta", "Ship.glb", "Rocks", "Lone.png.meta"].map(|name| from.join(name));

        let brought = bring_in(root, "Models", &dropped).unwrap();
        assert_eq!(brought, ["Models/Ship 1.glb", "Models/Rocks"], "a taken name is numbered");
        assert_eq!(std::fs::read_to_string(root.join("Models/Ship.glb")).unwrap(), "old", "nothing is replaced");
        assert!(root.join("Models/Ship 1.glb.meta").is_file(), "the .meta follows its file");
        assert!(root.join("Models/Rocks/Small/Pebble.glb").is_file(), "a folder comes with what is in it");
        assert!(!root.join("Models/Lone.png.meta").exists(), "a .meta without its file is left out");
    }
}
