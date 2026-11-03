//! `package.toml`: what a package says about itself.

use std::{collections::BTreeMap, path::Path};

use anyhow::{Context, Result, bail};
use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::source::Request;

/// The file at a package's root that makes a folder a package.
pub const MANIFEST_FILE: &str = "package.toml";

/// A package's `package.toml`:
///
/// ```toml
/// [package]
/// name = "orbit-tools"
/// version = "1.2.0"
/// description = "Cameras that orbit, follow and frame"
/// authors = ["Sam Rivers"]
/// engine = ">=0.1"
///
/// [dependencies]
/// orbit-core = "^1.0"
/// springs = { git = "https://github.com/someone/springs", tag = "v0.3.0" }
/// ```
#[derive(Clone, Debug)]
pub struct Manifest {
    pub name: String,
    pub version: Version,
    pub description: String,
    pub authors: Vec<String>,
    /// The engine versions the package works with; any, when it says none.
    pub engine: VersionReq,
    /// The packages it needs, by name, in the order written.
    pub dependencies: Vec<(String, Request)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    package: PackageInfo,
    #[serde(default)]
    dependencies: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageInfo {
    name: String,
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    engine: Option<String>,
}

impl Manifest {
    /// Reads the `package.toml` in a package's folder.
    pub fn read(folder: &Path) -> Result<Self> {
        let path = folder.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("{} is not a package: it has no {MANIFEST_FILE}", folder.display()))?;
        Self::parse(&text).with_context(|| format!("reading {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let file: ManifestFile = toml::from_str(text)?;
        let info = file.package;
        check_name(&info.name)?;
        let version = Version::parse(&info.version)
            .with_context(|| format!("`{}` is not a version such as 1.2.0", info.version))?;
        let engine = match &info.engine {
            Some(range) => {
                VersionReq::parse(range).with_context(|| format!("`{range}` is not a range of engine versions"))?
            }
            None => VersionReq::STAR,
        };
        let mut dependencies = Vec::new();
        for (name, value) in &file.dependencies {
            check_name(name)?;
            let request = Request::from_toml(value).with_context(|| format!("in the dependency on {name}"))?;
            dependencies.push((name.clone(), request));
        }
        Ok(Self {
            name: info.name,
            version,
            description: info.description,
            authors: info.authors,
            engine,
            dependencies,
        })
    }

    /// Whether the package works with this engine version. A pre-release
    /// engine, such as 0.2.0-dev, counts as the release it leads to.
    pub fn works_with(&self, engine: &Version) -> bool {
        works_with(&self.engine, engine)
    }
}

pub(crate) fn works_with(range: &VersionReq, engine: &Version) -> bool {
    let release = Version::new(engine.major, engine.minor, engine.patch);
    range.matches(&release)
}

/// A package's name: lower-case letters, digits and hyphens, from a letter,
/// as `orbit-tools`.
pub fn check_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let starts = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !starts || !rest || name.ends_with('-') || name.contains("--") {
        bail!("`{name}` is not a package name: use lower-case letters, digits and single hyphens, as orbit-tools");
    }
    Ok(())
}

/// The C# name a package's scripts are built as: `orbit-tools` is
/// `OrbitTools`.
pub fn assembly_name(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{GitRev, Request};

    #[test]
    fn reads_a_manifest() {
        let manifest = Manifest::parse(
            r#"
            [package]
            name = "orbit-tools"
            version = "1.2.0"
            description = "Cameras"
            authors = ["Sam"]
            engine = ">=0.1"

            [dependencies]
            orbit-core = "^1.0"
            springs = { git = "https://example.com/springs", tag = "v0.3.0" }
            near = { path = "../near" }
            "#,
        )
        .unwrap();
        assert_eq!(manifest.name, "orbit-tools");
        assert_eq!(manifest.version, Version::new(1, 2, 0));
        assert!(manifest.works_with(&Version::parse("0.1.0-dev").unwrap()));
        assert!(!manifest.works_with(&Version::new(0, 0, 9)));
        assert_eq!(manifest.dependencies.len(), 3);
        let springs = &manifest.dependencies.iter().find(|(name, _)| name == "springs").unwrap().1;
        assert!(matches!(springs, Request::Git { rev: GitRev::Tag(tag), .. } if tag == "v0.3.0"));
    }

    #[test]
    fn names_are_checked() {
        assert!(check_name("orbit-tools").is_ok());
        assert!(check_name("tools2").is_ok());
        for bad in ["Orbit", "2d", "orbit_tools", "orbit-", "a--b", ""] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
        assert_eq!(assembly_name("orbit-tools"), "OrbitTools");
    }
}
