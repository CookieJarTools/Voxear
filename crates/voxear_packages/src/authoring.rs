//! Making packages: starting one, and listing a tagged release of one in a
//! registry.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use semver::VersionReq;

use crate::{
    contents::{DOCS_FOLDER, SCRIPTS_FOLDER},
    manifest::{MANIFEST_FILE, Manifest, assembly_name, check_name, title},
    registry::{Release, add_release},
};

/// Starts a package in a new or empty folder: its `package.toml`, a README,
/// a script and a page of docs, for an engine of at least this version.
pub fn new_package(folder: &Path, name: &str, engine: &semver::Version) -> Result<()> {
    check_name(name)?;
    if folder.join(MANIFEST_FILE).exists() {
        bail!("{} is a package already", folder.display());
    }
    if std::fs::read_dir(folder).is_ok_and(|mut entries| entries.next().is_some()) {
        bail!("{} has files in it already: start a package in a new or empty folder", folder.display());
    }
    let namespace = assembly_name(name);
    let title = title(name);
    write(
        folder,
        MANIFEST_FILE,
        &format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\ndescription = \"\"\nauthors = []\nengine = \">={}.{}\"\n\n\
             [dependencies]\n",
            engine.major, engine.minor
        ),
    )?;
    write(
        folder,
        "README.md",
        &format!(
            "# {title}\n\nAdd it to a Voxear project with:\n\n```\nvox package add <this repository's URL>\n```\n"
        ),
    )?;
    write(
        folder,
        &format!("{SCRIPTS_FOLDER}/{namespace}.cs"),
        &format!(
            "using Voxear;\n\nnamespace {namespace};\n\n/// <summary>{title}'s first behaviour. Projects that add the package can put it on an Obj.</summary>\n\
             public sealed class {namespace}Behaviour : Behaviour\n{{\n\tprotected override void Start()\n\t{{\n\t\tLog.Info(\"{title} is here.\");\n\t}}\n}}\n"
        ),
    )?;
    write(
        folder,
        &format!("{DOCS_FOLDER}/{title}.md"),
        &format!("# {title}\n\nWhat the package is for, and how to use it.\n"),
    )?;
    Ok(())
}

fn write(folder: &Path, file: &str, text: &str) -> Result<()> {
    let path = folder.join(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// The release a package's folder is at: its repository's `origin`, the
/// commit checked out, which must be tagged and have nothing changed since,
/// and where the package is in the repository.
pub fn release_of(package: &Path) -> Result<(Manifest, Release)> {
    let manifest = Manifest::read(package)?;
    let changed = git(package, &["status", "--porcelain", "--", "."])?;
    if !changed.trim().is_empty() {
        bail!(
            "{} has changes that are not committed: commit them, tag the commit, and publish that",
            package.display()
        );
    }
    let commit = git(package, &["rev-parse", "HEAD"])?.trim().to_string();
    let tag = git(package, &["describe", "--tags", "--exact-match", "HEAD"])
        .map(|tag| tag.trim().to_string())
        .map_err(|_| {
            anyhow::anyhow!("the commit checked out has no tag: tag it, as v{}, and push the tag", manifest.version)
        })?;
    let git_url = git(package, &["remote", "get-url", "origin"]).map(|url| url.trim().to_string()).map_err(|_| {
        anyhow::anyhow!("the repository has no origin to fetch the package from: push it somewhere first")
    })?;
    let prefix = git(package, &["rev-parse", "--show-prefix"])?.trim().trim_end_matches('/').to_string();
    let release = Release {
        version: manifest.version.clone(),
        git: git_url,
        tag: Some(tag),
        commit,
        path: (!prefix.is_empty()).then_some(prefix),
        engine: manifest.engine.clone(),
        yanked: false,
    };
    Ok((manifest, release))
}

/// Lists the package's tagged release in a clone of a registry, to commit
/// and push or send as a pull request. Returns the file changed.
pub fn publish(package: &Path, registry: &Path) -> Result<PathBuf> {
    let (manifest, release) = release_of(package)?;
    if release.engine == VersionReq::STAR {
        tracing::warn!("{} says nothing of the engine versions it works with", manifest.name);
    }
    add_release(registry, &manifest.name, &manifest.description, &release)
}

fn git(folder: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .context("running git: is Git installed?")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_package_reads_back() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("orbit-tools");
        new_package(&root, "orbit-tools", &semver::Version::new(0, 1, 0)).unwrap();
        let manifest = Manifest::read(&root).unwrap();
        assert_eq!(manifest.name, "orbit-tools");
        assert!(manifest.works_with(&semver::Version::new(0, 1, 0)));
        assert!(root.join("Scripts/OrbitTools.cs").is_file());
        assert!(root.join("Docs/Orbit Tools.md").is_file());
        assert!(new_package(&root, "orbit-tools", &semver::Version::new(0, 1, 0)).is_err());
    }
}
