//! Where a package comes from, as `packages.toml` and `package.toml` write
//! it.

use std::{fmt, path::PathBuf};

use anyhow::{Context, Result, bail};
use semver::VersionReq;

/// A package asked for: from a Git repository, a folder, or a registry by
/// version.
///
/// ```toml
/// orbit-tools = { git = "https://github.com/someone/orbit-tools", tag = "v1.2.0" }
/// big-kit = { git = "https://github.com/someone/kits", branch = "main", path = "cameras" }
/// my-tool = { path = "../my-tool" }
/// cameras = "^1.0"
/// effects = { version = "^2", registry = "studio" }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Git {
        url: String,
        rev: GitRev,
        /// The package's folder in the repository, for one that holds more
        /// than one.
        path: Option<String>,
    },
    /// A folder on disk, from the folder of whoever asks for it.
    Folder { path: PathBuf },
    /// A version range from a registry: the default one, or one named in
    /// the project's `[registries]`.
    Registry { version: VersionReq, registry: Option<String> },
}

/// Which commit of a repository: a tag, the tip of a branch, an exact
/// commit, or the tip of the repository's default branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitRev {
    Tag(String),
    Branch(String),
    Commit(String),
    Default,
}

impl Request {
    pub fn from_toml(value: &toml::Value) -> Result<Self> {
        if let Some(range) = value.as_str() {
            return Ok(Self::Registry { version: parse_range(range)?, registry: None });
        }
        let Some(table) = value.as_table() else {
            bail!("write a version range such as \"^1.0\", or a table with git, path or version")
        };
        let text = |key: &str| -> Result<Option<String>> {
            match table.get(key) {
                None => Ok(None),
                Some(value) => match value.as_str() {
                    Some(text) => Ok(Some(text.to_string())),
                    None => bail!("`{key}` should be text"),
                },
            }
        };
        for key in table.keys() {
            if !["git", "tag", "branch", "rev", "path", "version", "registry"].contains(&key.as_str()) {
                bail!(
                    "`{key}` is not something a dependency has: use git, tag, branch, rev, path, version or registry"
                );
            }
        }
        if let Some(url) = text("git")? {
            let revs =
                [text("tag")?.map(GitRev::Tag), text("branch")?.map(GitRev::Branch), text("rev")?.map(GitRev::Commit)];
            let mut given = revs.into_iter().flatten();
            let rev = given.next().unwrap_or(GitRev::Default);
            if given.next().is_some() {
                bail!("give a Git dependency one of tag, branch or rev");
            }
            return Ok(Self::Git { url, rev, path: text("path")? });
        }
        if let Some(path) = text("path")? {
            return Ok(Self::Folder { path: PathBuf::from(path) });
        }
        if let Some(range) = text("version")? {
            return Ok(Self::Registry { version: parse_range(&range)?, registry: text("registry")? });
        }
        bail!("say where the package comes from, with git, path or version")
    }

    /// The request as `packages.toml` writes it.
    pub fn to_toml(&self) -> toml_edit::Item {
        let mut table = toml_edit::InlineTable::new();
        match self {
            Self::Registry { version, registry: None } => return toml_edit::value(version.to_string()),
            Self::Registry { version, registry: Some(registry) } => {
                table.insert("version", version.to_string().into());
                table.insert("registry", registry.as_str().into());
            }
            Self::Folder { path } => {
                table.insert("path", path.to_string_lossy().replace('\\', "/").into());
            }
            Self::Git { url, rev, path } => {
                table.insert("git", url.as_str().into());
                match rev {
                    GitRev::Tag(tag) => {
                        table.insert("tag", tag.as_str().into());
                    }
                    GitRev::Branch(branch) => {
                        table.insert("branch", branch.as_str().into());
                    }
                    GitRev::Commit(commit) => {
                        table.insert("rev", commit.as_str().into());
                    }
                    GitRev::Default => {}
                }
                if let Some(path) = path {
                    table.insert("path", path.as_str().into());
                }
            }
        }
        toml_edit::value(table)
    }
}

fn parse_range(range: &str) -> Result<VersionReq> {
    VersionReq::parse(range).with_context(|| format!("`{range}` is not a version range such as \"^1.2\""))
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Git { url, rev, path } => {
                write!(f, "{url}")?;
                match rev {
                    GitRev::Tag(tag) => write!(f, " at {tag}")?,
                    GitRev::Branch(branch) => write!(f, " on {branch}")?,
                    GitRev::Commit(commit) => write!(f, " at {}", short(commit))?,
                    GitRev::Default => {}
                }
                match path {
                    Some(path) => write!(f, ", in {path}"),
                    None => Ok(()),
                }
            }
            Self::Folder { path } => write!(f, "{}", path.display()),
            Self::Registry { version, registry: None } => write!(f, "{version}"),
            Self::Registry { version, registry: Some(registry) } => write!(f, "{version} from {registry}"),
        }
    }
}

/// A commit as Git shortens it, for messages.
pub fn short(commit: &str) -> &str {
    &commit[..commit.len().min(10)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str) -> Result<Request> {
        let value: toml::Value = toml::from_str(&format!("x = {text}")).unwrap();
        Request::from_toml(&value["x"])
    }

    #[test]
    fn requests_read_and_write_back() {
        for text in [
            r#""^1.0""#,
            r#"{ version = "^2", registry = "studio" }"#,
            r#"{ path = "../tool" }"#,
            r#"{ git = "https://example.com/a", tag = "v1.0.0" }"#,
            r#"{ git = "https://example.com/a", branch = "main", path = "cameras" }"#,
            r#"{ git = "https://example.com/a", rev = "abc123" }"#,
            r#"{ git = "https://example.com/a" }"#,
        ] {
            let read = request(text).unwrap();
            let written = read.to_toml().to_string();
            assert_eq!(request(&written).unwrap(), read, "{text} came back as {written}");
        }
    }

    #[test]
    fn mistakes_are_named() {
        assert!(request(r#"{ git = "a", tag = "v1", branch = "main" }"#).is_err());
        assert!(request(r#"{ gti = "a" }"#).unwrap_err().to_string().contains("gti"));
        assert!(request(r#""one point two""#).is_err());
    }
}
