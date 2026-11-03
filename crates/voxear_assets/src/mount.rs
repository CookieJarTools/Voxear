//! Packages' assets, seen beside the project's own.
//!
//! A package's assets are its files outside `Scripts/`, `Docs/` and
//! `Samples/`. Each package's are mounted under `Packages/<name>/`, so the
//! asset database, the content readers, bundles and the Project panel know
//! a package's model as `Packages/orbit-tools/Models/Rig.glb`, as Unity
//! knows its packages' assets. Which mount a path is in comes from the
//! [`Mounts`] the project has, by the package's name, never from the folder
//! the package sits in: a package from Git lives in the machine's shared
//! cache, which is somewhere else on every machine, and a path made from the
//! name is the same on all of them. References between assets are by GUID
//! in any case.
//!
//! A package from Git or a registry is [`Access::ReadOnly`]: its cache
//! folder is shared by every project on the machine and must never change.
//! - Its GUIDs come from the `.meta` files it ships. An asset without one
//!   gets a GUID made from the package's name and the asset's path in it,
//!   the same on every machine, so references to it still hold; a package
//!   made in Voxear ships its `.meta` files, since its author's editor wrote
//!   them while it was a folder package.
//! - Import settings the project changes for one of its assets are written
//!   to the project's `Settings/Packages/<name>/`, along the asset's path in
//!   the package (`Settings/Packages/orbit-tools/Models/Rig.glb.meta`), and
//!   read in place of the package's own `.meta`. They live in `Settings/`
//!   rather than `.voxear/`, because `.voxear/` is this machine's and kept
//!   out of version control, while everyone on a team, and every build
//!   machine, must import the package's assets the same way.
//!
//! A folder package is [`Access::Editable`]: it is the developer's own,
//! and its assets are treated as the project's, `.meta` files and all.

use std::path::{Path, PathBuf};

use crate::import::meta_path;

/// The folder packages' assets are mounted under.
pub const PACKAGES_FOLDER: &str = "Packages";
/// Where a project keeps the import settings it changed for packages it
/// cannot change, within its own folder.
pub const PACKAGE_SETTINGS_FOLDER: &str = "Settings/Packages";

/// Where an asset is: in the project, or in one of its packages.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mount {
    #[default]
    Project,
    /// A package's assets, by the package's name.
    Package(String),
}

impl Mount {
    /// The path the content knows a file in this mount by, from its path
    /// within the mount: the same for the project's, and under
    /// `Packages/<name>/` for a package's.
    pub fn content_path(&self, local: &str) -> String {
        match self {
            Self::Project => local.to_string(),
            Self::Package(name) if local.is_empty() => format!("{PACKAGES_FOLDER}/{name}"),
            Self::Package(name) => format!("{PACKAGES_FOLDER}/{name}/{local}"),
        }
    }

    /// The package's name, for a package's mount.
    pub fn package(&self) -> Option<&str> {
        match self {
            Self::Project => None,
            Self::Package(name) => Some(name),
        }
    }
}

/// Whether a mount's files may change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Editable,
    /// In the machine's package cache, which every project on it shares.
    ReadOnly,
}

/// A package whose assets are mounted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageMount {
    pub name: String,
    /// As the Project panel and VoxDocs show it: `Orbit Tools`.
    pub title: String,
    /// The package's folder, which holds its `package.toml`.
    pub root: PathBuf,
    pub access: Access,
}

impl PackageMount {
    pub fn mount(&self) -> Mount {
        Mount::Package(self.name.clone())
    }
}

/// The project's folder and the packages mounted beside it: what turns the
/// content's paths into files on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mounts {
    project: PathBuf,
    packages: Vec<PackageMount>,
}

impl Mounts {
    /// The project's folder alone.
    pub fn project(root: &Path) -> Self {
        Self { project: root.to_path_buf(), packages: Vec::new() }
    }

    pub fn new(root: &Path, packages: Vec<PackageMount>) -> Self {
        Self { project: root.to_path_buf(), packages }
    }

    /// The project's folder.
    pub fn root(&self) -> &Path {
        &self.project
    }

    pub fn packages(&self) -> &[PackageMount] {
        &self.packages
    }

    pub fn package(&self, name: &str) -> Option<&PackageMount> {
        self.packages.iter().find(|package| package.name == name)
    }

    /// The folder a mount's files are in.
    pub fn folder(&self, mount: &Mount) -> Option<&Path> {
        match mount {
            Mount::Project => Some(&self.project),
            Mount::Package(name) => self.package(name).map(|package| package.root.as_path()),
        }
    }

    pub fn access(&self, mount: &Mount) -> Access {
        match mount {
            Mount::Project => Access::Editable,
            Mount::Package(name) => self.package(name).map_or(Access::ReadOnly, |package| package.access),
        }
    }

    /// The mount a content path is in, and the path within it. Only a
    /// package that is mounted claims a path under `Packages/`; any other is
    /// the project's.
    pub fn locate<'a>(&self, path: &'a str) -> (Mount, &'a str) {
        let Some(rest) = path.strip_prefix(PACKAGES_FOLDER).and_then(|rest| rest.strip_prefix('/')) else {
            return (Mount::Project, path);
        };
        let (name, local) = rest.split_once('/').unwrap_or((rest, ""));
        match self.package(name) {
            Some(package) => (package.mount(), local),
            None => (Mount::Project, path),
        }
    }

    /// The file a content path is read from. A read-only package's `.meta`
    /// is read from the project's settings for it when the project changed
    /// its import settings there.
    pub fn file(&self, path: &str) -> PathBuf {
        let (mount, local) = self.locate(path);
        let Mount::Package(name) = &mount else { return self.project.join(path) };
        let package = self.package(name).expect("located in a mounted package");
        if package.access == Access::ReadOnly && local.ends_with(".meta") {
            let changed = self.settings_folder(name).join(local);
            if changed.is_file() {
                return changed;
            }
        }
        package.root.join(local)
    }

    /// The `.meta` the asset at a content path is read with: beside it, or
    /// the project's own for a read-only package's asset whose settings it
    /// changed.
    pub fn meta_file(&self, path: &str) -> PathBuf {
        self.file(&meta_path(Path::new(path)).to_string_lossy())
    }

    /// The `.meta` the asset at a content path has its import settings
    /// written to: beside it, or for a read-only package's asset, in the
    /// project's settings for the package, copied there from the package's
    /// own `.meta` the first time so that its other settings and its GUID
    /// stay as they are.
    pub fn writable_meta(&self, path: &str) -> anyhow::Result<PathBuf> {
        let (mount, local) = self.locate(path);
        let Mount::Package(name) = &mount else { return Ok(meta_path(&self.project.join(path))) };
        let package = self.package(name).expect("located in a mounted package");
        let own = meta_path(&package.root.join(local));
        if package.access == Access::Editable {
            return Ok(own);
        }
        let changed = meta_path(&self.settings_folder(name).join(local));
        if !changed.is_file() {
            std::fs::create_dir_all(changed.parent().unwrap_or(&self.project))?;
            std::fs::write(&changed, std::fs::read(&own).unwrap_or_default())?;
        }
        Ok(changed)
    }

    /// Where the project keeps the import settings it changed for a
    /// read-only package's assets.
    pub fn settings_folder(&self, package: &str) -> PathBuf {
        self.project.join(PACKAGE_SETTINGS_FOLDER).join(package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mounts(root: &Path, cache: &Path) -> Mounts {
        let package = |name: &str, folder: &str, access| PackageMount {
            name: name.to_string(),
            title: title(name),
            root: cache.join(folder),
            access,
        };
        Mounts::new(
            root,
            vec![package("orbit-tools", "orbit", Access::ReadOnly), package("my-tools", "mine", Access::Editable)],
        )
    }

    fn title(name: &str) -> String {
        name.replace('-', " ")
    }

    #[test]
    fn paths_under_packages_are_found_by_the_package_name() {
        let (project, cache) = (Path::new("/game"), Path::new("/cache"));
        let mounts = mounts(project, cache);
        let rig = "Packages/orbit-tools/Models/Rig.glb";
        assert_eq!(mounts.locate(rig), (Mount::Package("orbit-tools".into()), "Models/Rig.glb"));
        assert_eq!(mounts.file(rig), cache.join("orbit/Models/Rig.glb"));
        assert_eq!(mounts.locate("Packages/unknown/A.glb"), (Mount::Project, "Packages/unknown/A.glb"));
        assert_eq!(mounts.locate("Models/Ship.glb"), (Mount::Project, "Models/Ship.glb"));
        assert_eq!(Mount::Package("orbit-tools".into()).content_path("Models/Rig.glb"), rig);
        assert_eq!(Mount::Package("orbit-tools".into()).content_path(""), "Packages/orbit-tools");
    }

    #[test]
    fn a_cached_package_has_its_import_settings_written_into_the_project() {
        let (project, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let mounts = mounts(project.path(), cache.path());
        std::fs::create_dir_all(cache.path().join("orbit/Models")).unwrap();
        std::fs::write(cache.path().join("orbit/Models/Rig.glb.meta"), "guid = \"abc\"\n").unwrap();
        let rig = "Packages/orbit-tools/Models/Rig.glb";
        assert_eq!(mounts.meta_file(rig), cache.path().join("orbit/Models/Rig.glb.meta"));

        let written = mounts.writable_meta(rig).unwrap();
        assert_eq!(written, project.path().join("Settings/Packages/orbit-tools/Models/Rig.glb.meta"));
        assert_eq!(std::fs::read_to_string(&written).unwrap(), "guid = \"abc\"\n", "copied from the package's");
        assert_eq!(mounts.meta_file(rig), written, "and read in its place from then on");

        let mine = "Packages/my-tools/Rock.png";
        assert_eq!(mounts.writable_meta(mine).unwrap(), cache.path().join("mine/Rock.png.meta"), "a folder package's");
    }
}
