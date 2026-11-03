//! `packages.toml`: the packages a project asks for, at its root beside
//! `voxear.toml`.
//!
//! ```toml
//! [packages]
//! orbit-tools = { git = "https://github.com/someone/orbit-tools", tag = "v1.2.0" }
//! my-tool = { path = "../my-tool" }
//! cameras = "^1.0"
//!
//! [registries]
//! studio = "https://github.com/my-studio/packages"
//! ```
//!
//! Changes made through Voxear keep the file's comments and layout.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    manifest::check_name,
    registry::{DEFAULT_REGISTRY, Registry},
    resolve::Registries,
    source::Request,
};

pub const PACKAGES_FILE: &str = "packages.toml";

/// Names another default registry for the whole machine, for studios with
/// their own.
pub const REGISTRY_VARIABLE: &str = "VOXEAR_REGISTRY";

pub struct PackagesFile {
    path: PathBuf,
    document: toml_edit::DocumentMut,
}

impl PackagesFile {
    /// A project's `packages.toml`, empty when it has none yet.
    pub fn open(project: &Path) -> Result<Self> {
        let path = project.join(PACKAGES_FILE);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let document = text.parse().with_context(|| format!("reading {}", path.display()))?;
        let file = Self { path, document };
        for key in file.document.as_table().iter().map(|(key, _)| key) {
            if key != "packages" && key != "registries" {
                bail!("{} has `{key}`, but only [packages] and [registries] belong there", file.path.display());
            }
        }
        Ok(file)
    }

    /// What the project asks for, in the order written.
    pub fn requests(&self) -> Result<Vec<(String, Request)>> {
        let Some(table) = self.document.get("packages").and_then(|item| item.as_table_like()) else {
            return Ok(Vec::new());
        };
        let mut requests = Vec::new();
        for (name, item) in table.iter() {
            check_name(name)?;
            let value = item_value(item).with_context(|| format!("{name} in {}", self.path.display()))?;
            let request = Request::from_toml(&value).with_context(|| format!("{name} in {}", self.path.display()))?;
            requests.push((name.to_string(), request));
        }
        Ok(requests)
    }

    pub fn registries(&self) -> Result<Registries> {
        let default = std::env::var(REGISTRY_VARIABLE).unwrap_or_else(|_| DEFAULT_REGISTRY.to_string());
        let mut registries = Registries { default: Registry::new(default), named: BTreeMap::new() };
        let Some(table) = self.document.get("registries").and_then(|item| item.as_table_like()) else {
            return Ok(registries);
        };
        for (name, item) in table.iter() {
            let url = item.as_str().with_context(|| format!("the registry {name} should be a URL"))?;
            let registry = Registry::new(url);
            if name == "default" {
                registries.default = registry;
            } else {
                registries.named.insert(name.to_string(), registry);
            }
        }
        Ok(registries)
    }

    /// Asks for a package, or changes how it is asked for.
    pub fn set(&mut self, name: &str, request: &Request) -> Result<()> {
        check_name(name)?;
        let table = self
            .document
            .entry("packages")
            .or_insert(toml_edit::table())
            .as_table_mut()
            .context("[packages] should be a table")?;
        table[name] = request.to_toml();
        Ok(())
    }

    /// Stops asking for a package; false when it was not asked for.
    pub fn remove(&mut self, name: &str) -> bool {
        let Some(table) = self.document.get_mut("packages").and_then(|item| item.as_table_like_mut()) else {
            return false;
        };
        table.remove(name).is_some()
    }

    pub fn has(&self, name: &str) -> bool {
        self.document
            .get("packages")
            .and_then(|item| item.as_table_like())
            .is_some_and(|table| table.contains_key(name))
    }

    /// Writes the file, unless it holds this already: every build of the
    /// scripts syncs the packages, and a file written again for nothing
    /// would look like a change to whatever watches the project.
    pub fn save(&self) -> Result<()> {
        let text = self.document.to_string();
        if std::fs::read_to_string(&self.path).is_ok_and(|known| known == text) {
            return Ok(());
        }
        std::fs::write(&self.path, text).with_context(|| format!("writing {}", self.path.display()))
    }
}

/// An item of the document as a plain value, for reading with serde.
fn item_value(item: &toml_edit::Item) -> Result<toml::Value> {
    let text = match item {
        toml_edit::Item::Value(value) => format!("x = {value}"),
        toml_edit::Item::Table(table) => format!("[x]\n{table}"),
        _ => bail!("write a version range or a table"),
    };
    let mut table: toml::Table = toml::from_str(&text)?;
    table.remove("x").context("reading the request")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_keep_comments() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(PACKAGES_FILE),
            "# Our packages\n[packages]\ncameras = \"^1.0\" # the good ones\n\n[packages.tool]\npath = \"../tool\"\n",
        )
        .unwrap();
        let mut file = PackagesFile::open(folder.path()).unwrap();
        let requests = file.requests().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].1, Request::Folder { path: "../tool".into() });
        file.set("springs", &Request::Folder { path: "../springs".into() }).unwrap();
        assert!(file.remove("tool"));
        file.save().unwrap();
        let text = std::fs::read_to_string(folder.path().join(PACKAGES_FILE)).unwrap();
        assert!(text.contains("# Our packages") && text.contains("# the good ones"), "{text}");
        let again = PackagesFile::open(folder.path()).unwrap().requests().unwrap();
        let names: Vec<&str> = again.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["cameras", "springs"]);
    }

    #[test]
    fn registries_are_named() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(PACKAGES_FILE), "[registries]\nstudio = \"https://example.com/reg\"\n")
            .unwrap();
        let registries = PackagesFile::open(folder.path()).unwrap().registries().unwrap();
        assert_eq!(registries.get(Some("studio")).unwrap().url, "https://example.com/reg");
        assert!(registries.get(Some("other")).is_err());
    }
}
