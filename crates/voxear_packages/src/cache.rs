//! The machine's copy of every package fetched from Git, shared by all its
//! projects: one bare repository per URL, and one folder per commit used.
//!
//! Git does the fetching, through the `git` command, so the credentials,
//! proxies and SSH keys a developer already has for it work as they are.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use crate::source::{GitRev, short};

/// Names another folder for the cache, as tests and build machines do.
pub const CACHE_VARIABLE: &str = "VOXEAR_PACKAGE_CACHE";

/// Whether looking up a commit may go to the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// Only for what the cache does not have yet: a tag already fetched is
    /// taken as it is, and a branch moves only when asked.
    IfMissing,
    /// Always, to see where branches and tags are now, as an update does.
    Always,
    /// Never: what the cache has, or an error.
    Offline,
}

#[derive(Clone, Debug)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// The cache for this machine: `VOXEAR_PACKAGE_CACHE` when it is set, or
    /// else the user's cache folder, `~/.cache/voxear/packages` on Linux and
    /// `%LOCALAPPDATA%\Voxear\Packages` on Windows.
    pub fn for_machine() -> Self {
        if let Some(folder) = std::env::var_os(CACHE_VARIABLE) {
            return Self::at(folder);
        }
        if cfg!(windows) {
            let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
            return Self::at(local.join("Voxear").join("Packages"));
        }
        let cache = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .unwrap_or_else(std::env::temp_dir);
        Self::at(cache.join("voxear").join("packages"))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The commit a revision of a repository is at.
    pub fn find(&self, url: &str, rev: &GitRev, fetch: Fetch) -> Result<String> {
        let repository = self.repository(url);
        let reference = match rev {
            GitRev::Tag(tag) => format!("refs/tags/{tag}"),
            GitRev::Branch(branch) => format!("refs/heads/{branch}"),
            GitRev::Commit(commit) => commit.clone(),
            GitRev::Default => "refs/voxear/default".to_string(),
        };
        let settled = matches!(rev, GitRev::Tag(_) | GitRev::Commit(_));
        if fetch == Fetch::Always || (fetch == Fetch::IfMissing && !settled) {
            self.fetch(url)?;
        }
        if let Some(commit) = self.commit_of(&repository, &reference) {
            return Ok(commit);
        }
        if fetch == Fetch::Offline {
            bail!("{url} has not been fetched to this machine yet, and packages are offline");
        }
        if settled {
            self.fetch(url)?;
            if let Some(commit) = self.commit_of(&repository, &reference) {
                return Ok(commit);
            }
        }
        match rev {
            GitRev::Tag(tag) => bail!("{url} has no tag {tag}"),
            GitRev::Branch(branch) => bail!("{url} has no branch {branch}"),
            GitRev::Commit(commit) => bail!("{url} has no commit {}", short(commit)),
            GitRev::Default => bail!("{url} has no default branch"),
        }
    }

    /// A folder holding the repository as it was at a commit, which is made
    /// once and then shared. Nothing should change it.
    pub fn checkout(&self, url: &str, commit: &str, fetch: Fetch) -> Result<PathBuf> {
        let folder = self.root.join("checkouts").join(slug(url)).join(commit);
        if folder.is_dir() {
            return Ok(folder);
        }
        let repository = self.repository(url);
        if self.commit_of(&repository, commit).is_none() {
            if fetch == Fetch::Offline {
                bail!("{url} at {} is not on this machine yet, and packages are offline", short(commit));
            }
            self.fetch(url)?;
        }
        let parent = folder.parent().context("a checkout has a parent")?;
        std::fs::create_dir_all(parent)?;
        let making = parent.join(format!(".{commit}.{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&making);
        std::fs::create_dir_all(&making)?;
        extract(&repository, commit, &making).with_context(|| format!("unpacking {url} at {}", short(commit)))?;
        if std::fs::rename(&making, &folder).is_err() {
            // Another process made the same checkout first; theirs is as good.
            std::fs::remove_dir_all(&making)?;
            if !folder.is_dir() {
                bail!("could not keep {url} at {} in {}", short(commit), folder.display());
            }
        }
        Ok(folder)
    }

    /// Brings the machine's copy of a repository up to date with it.
    pub fn fetch(&self, url: &str) -> Result<()> {
        let repository = self.repository(url);
        if !repository.join("HEAD").is_file() {
            std::fs::create_dir_all(&repository)?;
            git(&repository, &["init", "--bare", "--quiet"])?;
        }
        tracing::info!("fetching {url}");
        let refs = ["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*", "+HEAD:refs/voxear/default"];
        let mut args = vec!["fetch", "--quiet", "--no-write-fetch-head", url];
        args.extend(refs);
        git(&repository, &args).with_context(|| format!("fetching {url}"))
    }

    fn repository(&self, url: &str) -> PathBuf {
        self.root.join("git").join(slug(url))
    }

    fn commit_of(&self, repository: &Path, reference: &str) -> Option<String> {
        if !repository.join("HEAD").is_file() {
            return None;
        }
        let output = git_output(repository, &["rev-parse", "--verify", "--quiet", &format!("{reference}^{{commit}}")]);
        output.ok().map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
    }
}

/// A folder name for a URL that a person can recognise, made unique by a
/// hash of the whole URL.
fn slug(url: &str) -> String {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    let last = trimmed.rsplit(['/', '\\', ':']).next().unwrap_or("repository");
    let name: String = last.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{name}-{hash:016x}")
}

fn extract(repository: &Path, commit: &str, into: &Path) -> Result<()> {
    let mut child = command(repository)
        .args(["archive", "--format=tar", commit])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running git: is Git installed?")?;
    let stdout = child.stdout.take().context("git archive's output")?;
    let unpacked = tar::Archive::new(stdout).unpack(into);
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    unpacked.context("unpacking git archive's output")
}

fn command(repository: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("--git-dir").arg(repository).env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::null());
    command
}

fn git(repository: &Path, args: &[&str]) -> Result<()> {
    git_output(repository, args).map(drop)
}

fn git_output(repository: &Path, args: &[&str]) -> Result<String> {
    let output = command(repository).args(args).output().context("running git: is Git installed?")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_readable_and_unique() {
        let a = slug("https://github.com/someone/orbit-tools.git");
        let b = slug("https://github.com/other/orbit-tools");
        assert!(a.starts_with("orbit-tools-"), "{a}");
        assert_ne!(a, b);
        assert!(slug("/home/me/My Tool").starts_with("My_Tool-"));
    }
}
