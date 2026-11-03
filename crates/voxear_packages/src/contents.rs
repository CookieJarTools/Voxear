//! What a package holds, by the folders it is laid out in:
//!
//! - `Scripts/`: C#, built into an assembly of the package's own that the
//!   game's scripts can use, with any editor windows, side bar items,
//!   drawers and commands it has;
//! - `Docs/`: VoxDocs pages, shown beside the manual;
//! - `Samples/<name>/`: samples a project can copy in and change;
//! - anything else: assets, which projects use where they are.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::manifest::title;

pub const SCRIPTS_FOLDER: &str = "Scripts";
pub const DOCS_FOLDER: &str = "Docs";
pub const SAMPLES_FOLDER: &str = "Samples";

/// Files at a package's top that are about the package, not assets in it.
const ABOUT: [&str; 4] = ["README", "LICENSE", "CHANGELOG", "package"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents {
    /// Its C# files, which run when the project's scripts do.
    pub scripts: Vec<PathBuf>,
    pub docs: usize,
    pub assets: usize,
    pub samples: Vec<Sample>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub name: String,
    pub folder: PathBuf,
}

impl Contents {
    pub fn of(root: &Path) -> Self {
        let mut contents = Self::default();
        let mut scripts = Vec::new();
        files(&root.join(SCRIPTS_FOLDER), &mut scripts);
        contents.scripts = scripts.into_iter().filter(|path| path.extension().is_some_and(|ext| ext == "cs")).collect();
        let mut docs = Vec::new();
        files(&root.join(DOCS_FOLDER), &mut docs);
        contents.docs = docs.iter().filter(|path| path.extension().is_some_and(|ext| ext == "md")).count();
        if let Ok(entries) = std::fs::read_dir(root.join(SAMPLES_FOLDER)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.path().is_dir() && !name.starts_with('.') {
                    contents.samples.push(Sample { name, folder: entry.path() });
                }
            }
        }
        contents.samples.sort_by(|a, b| a.name.cmp(&b.name));
        let mut assets = Vec::new();
        for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if ![SCRIPTS_FOLDER, DOCS_FOLDER, SAMPLES_FOLDER].contains(&name.as_str()) {
                    files(&path, &mut assets);
                }
            } else if !is_about(&name) {
                assets.push(path);
            }
        }
        contents.assets = assets.iter().filter(|path| path.extension().is_none_or(|ext| ext != "meta")).count();
        contents
    }

    /// Whether adding it runs code of someone else's.
    pub fn runs_code(&self) -> bool {
        !self.scripts.is_empty()
    }
}

/// Where a project keeps the samples it copied from packages.
pub const PROJECT_SAMPLES_FOLDER: &str = "Samples";

impl Sample {
    /// Where the sample goes in a project: `Samples/Orbit Tools/First
    /// Orbit` for orbit-tools' First Orbit.
    pub fn destination(&self, project: &Path, package: &str) -> PathBuf {
        project.join(PROJECT_SAMPLES_FOLDER).join(title(package)).join(&self.name)
    }

    /// Copies the sample into a project, to change as its own. Returns
    /// where it went; a sample already copied is left as it is.
    pub fn import(&self, project: &Path, package: &str) -> Result<PathBuf> {
        let destination = self.destination(project, package);
        if destination.exists() {
            bail!("{} is in the project already, at {}", self.name, destination.display());
        }
        copy_folder(&self.folder, &destination)
            .with_context(|| format!("copying {} into {}", self.name, destination.display()))?;
        Ok(destination)
    }
}

fn copy_folder(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_folder(&path, &to.join(&name))?;
        } else {
            std::fs::copy(&path, to.join(&name))?;
        }
    }
    Ok(())
}

fn is_about(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    name.starts_with('.') || ABOUT.iter().any(|about| stem.eq_ignore_ascii_case(about))
}

/// Every file under a folder, leaving out hidden ones and build output.
fn files(folder: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "bin" || name == "obj" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            files(&path, into);
        } else {
            into.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contents_are_counted_by_folder() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        for file in [
            "package.toml",
            "README.md",
            "LICENSE",
            "Scripts/Orbit.cs",
            "Scripts/Editor/OrbitWindow.cs",
            "Scripts/obj/Generated.cs",
            "Docs/Orbit Cameras.md",
            "Samples/First Orbit/Orbit.level",
            "Models/Rig.glb",
            "Models/Rig.glb.meta",
            ".git/HEAD",
            "icon.png",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        let contents = Contents::of(root);
        assert_eq!(contents.scripts.len(), 2);
        assert!(contents.runs_code());
        assert_eq!(contents.docs, 1);
        assert_eq!(contents.assets, 2);
        assert_eq!(contents.samples.len(), 1);
        assert_eq!(contents.samples[0].name, "First Orbit");

        let project = tempfile::tempdir().unwrap();
        let copied = contents.samples[0].import(project.path(), "orbit-tools").unwrap();
        assert_eq!(copied, project.path().join("Samples/Orbit Tools/First Orbit"));
        assert!(copied.join("Orbit.level").is_file());
        assert!(contents.samples[0].import(project.path(), "orbit-tools").is_err());
    }
}
