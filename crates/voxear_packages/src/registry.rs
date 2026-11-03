//! Registries: plain Git repositories that list packages and their
//! releases, for finding a package by its name.
//!
//! A registry holds a file for each package, `packages/<name>.toml`:
//!
//! ```toml
//! description = "Cameras that orbit, follow and frame"
//!
//! [[version]]
//! version = "1.2.0"
//! git = "https://github.com/someone/orbit-tools"
//! tag = "v1.2.0"
//! commit = "4f0c1a7e9b2d…"
//! engine = ">=0.1"
//! ```
//!
//! The commit is what is fetched; the tag is there for people reading it.
//! A release marked `yanked = true` is never picked again, but projects
//! already locked to it keep it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::{
    cache::{Cache, Fetch},
    manifest::{check_name, works_with},
    source::GitRev,
};

/// The registry packages come from when a project names no other.
pub const DEFAULT_REGISTRY: &str = "https://github.com/CookieJarTools/VoxearPackages";

/// Where a registry keeps its packages' files.
pub const REGISTRY_FOLDER: &str = "packages";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registry {
    pub url: String,
}

/// A package as a registry lists it.
#[derive(Clone, Debug)]
pub struct Listing {
    pub name: String,
    pub description: String,
    /// Its releases, oldest first.
    pub releases: Vec<Release>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub git: String,
    pub tag: Option<String>,
    pub commit: String,
    /// The package's folder in its repository, for one that holds more.
    pub path: Option<String>,
    pub engine: VersionReq,
    pub yanked: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListingFile {
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: Vec<ReleaseEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseEntry {
    version: String,
    git: String,
    #[serde(default)]
    tag: Option<String>,
    commit: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    yanked: bool,
}

impl Listing {
    fn parse(name: &str, text: &str) -> Result<Self> {
        let file: ListingFile = toml::from_str(text)?;
        let mut releases = Vec::new();
        for entry in file.version {
            let version =
                Version::parse(&entry.version).with_context(|| format!("`{}` is not a version", entry.version))?;
            let engine = match entry.engine {
                Some(range) => VersionReq::parse(&range).with_context(|| format!("`{range}` is not a range"))?,
                None => VersionReq::STAR,
            };
            releases.push(Release {
                version,
                git: entry.git,
                tag: entry.tag,
                commit: entry.commit,
                path: entry.path,
                engine,
                yanked: entry.yanked,
            });
        }
        releases.sort_by(|a, b| a.version.cmp(&b.version));
        Ok(Self { name: name.to_string(), description: file.description, releases })
    }

    /// The newest release that every range allows and that works with the
    /// engine, leaving out yanked ones.
    pub fn best(&self, ranges: &[&VersionReq], engine: &Version) -> Option<&Release> {
        self.releases
            .iter()
            .rev()
            .filter(|release| !release.yanked && works_with(&release.engine, engine))
            .find(|release| ranges.iter().all(|range| range.matches(&release.version)))
    }

    /// The newest release that works with the engine.
    pub fn latest(&self, engine: &Version) -> Option<&Release> {
        self.best(&[], engine)
    }
}

impl Registry {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// The registry's files as they are now, or as the cache last saw them.
    pub fn index(&self, cache: &Cache, fetch: Fetch) -> Result<PathBuf> {
        let commit = cache
            .find(&self.url, &GitRev::Default, fetch)
            .with_context(|| format!("reading the registry {}", self.url))?;
        cache.checkout(&self.url, &commit, fetch)
    }

    /// A package by its name, or `None` when the registry does not list it.
    pub fn listing(&self, cache: &Cache, name: &str, fetch: Fetch) -> Result<Option<Listing>> {
        check_name(name)?;
        let file = self.index(cache, fetch)?.join(REGISTRY_FOLDER).join(format!("{name}.toml"));
        let Ok(text) = std::fs::read_to_string(&file) else { return Ok(None) };
        let listing = Listing::parse(name, &text).with_context(|| format!("reading {name} in {}", self.url))?;
        Ok(Some(listing))
    }

    /// The packages whose names or descriptions hold every word of a
    /// search, ignoring case; all of them for an empty one.
    pub fn search(&self, cache: &Cache, query: &str, fetch: Fetch) -> Result<Vec<Listing>> {
        let folder = self.index(cache, fetch)?.join(REGISTRY_FOLDER);
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(&folder) else { return Ok(found) };
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(listing) = read_listing(&path) {
                let text = format!("{} {}", listing.name, listing.description).to_lowercase();
                if words.iter().all(|word| text.contains(word)) {
                    found.push(listing);
                }
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }
}

fn read_listing(path: &Path) -> Option<Listing> {
    if path.extension()? != "toml" {
        return None;
    }
    let name = path.file_stem()?.to_str()?;
    check_name(name).ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    match Listing::parse(name, &text) {
        Ok(listing) => Some(listing),
        Err(error) => {
            tracing::warn!("skipping {}: {error:#}", path.display());
            None
        }
    }
}

/// Adds a release to a package's file in a registry's folder, a clone of
/// the registry to commit and push, or to send as a pull request. Returns
/// the file written.
pub fn add_release(registry: &Path, name: &str, description: &str, release: &Release) -> Result<PathBuf> {
    check_name(name)?;
    let folder = registry.join(REGISTRY_FOLDER);
    std::fs::create_dir_all(&folder)?;
    let file = folder.join(format!("{name}.toml"));
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    if !text.is_empty() {
        let listing = Listing::parse(name, &text).with_context(|| format!("reading {}", file.display()))?;
        if listing.releases.iter().any(|known| known.version == release.version) {
            bail!("{name} {} is in the registry already: give the package a new version", release.version);
        }
    }
    let mut document: toml_edit::DocumentMut = text.parse().with_context(|| format!("reading {}", file.display()))?;
    if !description.is_empty() {
        document["description"] = toml_edit::value(description);
    }
    let mut entry = toml_edit::Table::new();
    entry["version"] = toml_edit::value(release.version.to_string());
    entry["git"] = toml_edit::value(&release.git);
    if let Some(tag) = &release.tag {
        entry["tag"] = toml_edit::value(tag);
    }
    entry["commit"] = toml_edit::value(&release.commit);
    if let Some(path) = &release.path {
        entry["path"] = toml_edit::value(path);
    }
    if release.engine != VersionReq::STAR {
        entry["engine"] = toml_edit::value(release.engine.to_string());
    }
    let releases = document
        .entry("version")
        .or_insert(toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .context("`version` in the registry's file should be a list of [[version]] tables")?;
    releases.push(entry);
    std::fs::write(&file, document.to_string()).with_context(|| format!("writing {}", file.display()))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"
        description = "Cameras"

        [[version]]
        version = "1.0.0"
        git = "https://example.com/cams"
        commit = "aaa"

        [[version]]
        version = "1.3.0"
        git = "https://example.com/cams"
        commit = "ccc"
        engine = ">=0.2"

        [[version]]
        version = "1.2.0"
        git = "https://example.com/cams"
        commit = "bbb"

        [[version]]
        version = "2.0.0"
        git = "https://example.com/cams"
        commit = "ddd"
        yanked = true
    "#;

    #[test]
    fn the_newest_release_that_fits_is_picked() {
        let listing = Listing::parse("cams", LISTING).unwrap();
        let engine = Version::new(0, 1, 0);
        let any = VersionReq::parse("^1").unwrap();
        // 1.3.0 needs a newer engine and 2.0.0 was yanked.
        assert_eq!(listing.best(&[&any], &engine).unwrap().commit, "bbb");
        let old = VersionReq::parse("<1.2").unwrap();
        assert_eq!(listing.best(&[&any, &old], &engine).unwrap().commit, "aaa");
        assert_eq!(listing.best(&[&any], &Version::new(0, 2, 0)).unwrap().commit, "ccc");
        assert!(listing.best(&[&VersionReq::parse("^3").unwrap()], &engine).is_none());
    }

    #[test]
    fn releases_are_added_once() {
        let folder = tempfile::tempdir().unwrap();
        let release = Release {
            version: Version::new(1, 0, 0),
            git: "https://example.com/cams".into(),
            tag: Some("v1.0.0".into()),
            commit: "aaa".into(),
            path: None,
            engine: VersionReq::parse(">=0.1").unwrap(),
            yanked: false,
        };
        let file = add_release(folder.path(), "cams", "Cameras", &release).unwrap();
        let next = Release { version: Version::new(1, 1, 0), commit: "bbb".into(), ..release.clone() };
        add_release(folder.path(), "cams", "", &next).unwrap();
        assert!(add_release(folder.path(), "cams", "", &release).is_err());
        let listing = Listing::parse("cams", &std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(listing.description, "Cameras");
        assert_eq!(listing.releases.len(), 2);
        assert_eq!(listing.releases[0], release);
    }
}
