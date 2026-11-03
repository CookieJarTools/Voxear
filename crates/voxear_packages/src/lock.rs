//! `packages.lock`: the exact packages a project resolved to, so that
//! everyone on a team, and every build, gets the same code. It is written
//! by Voxear and meant to be committed with the project.

use std::path::Path;

use anyhow::{Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{
    resolve::{Origin, Package},
    source::GitRev,
};

pub const LOCK_FILE: &str = "packages.lock";

const HEADER: &str = "# Written by Voxear: the exact packages this project uses. Commit it with the project, and\n\
                      # change packages.toml, not this.\n\n";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lock {
    pub packages: Vec<Locked>,
}

/// One package as it was resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locked {
    pub name: String,
    pub version: String,
    /// A Git package's repository, or a registry package's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The commit asked for, for a Git package pinned to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// The package's folder in its repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// A folder package's folder, as it was asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The registry a package came from by version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct LockFile {
    #[serde(default, rename = "package")]
    packages: Vec<Locked>,
}

impl Lock {
    /// A project's lock, empty when it has none yet.
    pub fn read(project: &Path) -> Result<Self> {
        let path = project.join(LOCK_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else { return Ok(Self::default()) };
        let file: LockFile = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        Ok(Self { packages: file.packages })
    }

    /// Writes the lock, or takes it away when the project has no packages.
    pub fn write(&self, project: &Path) -> Result<()> {
        let path = project.join(LOCK_FILE);
        if self.packages.is_empty() {
            if path.exists() {
                std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            }
            return Ok(());
        }
        let file = LockFile { packages: self.packages.clone() };
        let text = format!("{HEADER}{}", toml::to_string(&file)?);
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    pub fn get(&self, name: &str) -> Option<&Locked> {
        self.packages.iter().find(|locked| locked.name == name)
    }

    pub fn of(packages: &[Package]) -> Self {
        Self { packages: packages.iter().map(Locked::of).collect() }
    }
}

impl Locked {
    pub fn of(package: &Package) -> Self {
        let mut locked = Self {
            name: package.name.clone(),
            version: package.manifest.version.to_string(),
            git: None,
            tag: None,
            branch: None,
            rev: None,
            path: None,
            folder: None,
            registry: None,
            commit: None,
            dependencies: package.dependencies.clone(),
        };
        match &package.origin {
            Origin::Git { url, rev, path, commit } => {
                locked.git = Some(url.clone());
                match rev {
                    GitRev::Tag(tag) => locked.tag = Some(tag.clone()),
                    GitRev::Branch(branch) => locked.branch = Some(branch.clone()),
                    GitRev::Commit(asked) => locked.rev = Some(asked.clone()),
                    GitRev::Default => {}
                }
                locked.path = path.clone();
                locked.commit = Some(commit.clone());
            }
            Origin::Folder { asked, .. } => {
                locked.folder = Some(asked.to_string_lossy().replace('\\', "/"));
            }
            Origin::Registry { registry, release } => {
                locked.registry = Some(registry.clone());
                locked.git = Some(release.git.clone());
                locked.tag = release.tag.clone();
                locked.path = release.path.clone();
                locked.commit = Some(release.commit.clone());
            }
        }
        locked
    }

    pub fn version(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }

    /// The commit a Git package locked to, when it was asked for as it is
    /// asked for now.
    pub(crate) fn git_commit(&self, url: &str, rev: &GitRev, path: &Option<String>) -> Option<&str> {
        let (tag, branch, asked) = match rev {
            GitRev::Tag(tag) => (Some(tag), None, None),
            GitRev::Branch(branch) => (None, Some(branch), None),
            GitRev::Commit(commit) => (None, None, Some(commit)),
            GitRev::Default => (None, None, None),
        };
        let same = self.registry.is_none()
            && self.git.as_deref() == Some(url)
            && self.tag.as_ref() == tag
            && self.branch.as_ref() == branch
            && self.rev.as_ref() == asked
            && &self.path == path;
        if same { self.commit.as_deref() } else { None }
    }
}
