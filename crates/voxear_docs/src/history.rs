//! When pages and the files they link to last changed, as git has it, for
//! Docs Health: a page is out of date when a file it links to changed on a
//! later day than the page did. Git's date holds however the page was
//! edited, where the date in its header moves only when VoxDocs saves it.
//!
//! A file with changes not committed yet changed when the file says; a path
//! git does not know, or a project not in git, falls back to the same.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    process::Command,
};

use crate::header;

#[derive(Debug, Default)]
pub struct History {
    /// Each path's last commit, as a date, by its path from the project.
    committed: HashMap<String, String>,
    /// Paths with changes not committed, and those git does not track.
    uncommitted: HashSet<String>,
}

impl History {
    /// What git knows of `paths`, each from the project's folder, in one
    /// look through the history. Nothing when the project is not in git.
    pub fn read(project: &Path, paths: &[String]) -> Self {
        let mut history = History::default();
        if paths.is_empty() {
            return history;
        }
        let git = |args: &[&str]| {
            let output = Command::new("git").arg("-C").arg(project).args(args).args(paths).output().ok()?;
            output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        };
        let Some(log) = git(&["log", "--format=%x00%ct", "--name-only", "--relative", "--"]) else { return history };
        // Newest first, so a path's first commit here is its last.
        for commit in log.split('\0').filter(|c| !c.trim().is_empty()) {
            let mut lines = commit.lines();
            let Some(seconds) = lines.next().and_then(|line| line.trim().parse::<u64>().ok()) else { continue };
            let date = header::date_of(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds));
            for path in lines.map(str::trim).filter(|path| !path.is_empty()) {
                history.committed.entry(path.to_string()).or_insert_with(|| date.clone());
            }
        }
        if let Some(changed) = git(&["ls-files", "--modified", "--others", "--exclude-standard", "--"]) {
            history.uncommitted = changed.lines().map(str::to_string).collect();
        }
        history
    }

    /// The day `path` last changed: its last commit, or for one with changes
    /// not committed, or git does not know, the file's own time.
    pub fn changed(&self, project: &Path, path: &str) -> Option<String> {
        if !self.uncommitted.contains(path)
            && let Some(date) = self.committed.get(path)
        {
            return Some(date.clone());
        }
        let modified = std::fs::metadata(project.join(path)).and_then(|m| m.modified()).ok()?;
        Some(header::date_of(modified))
    }

    /// Whether git has a commit of `path`, as it has for a page once it is
    /// committed.
    pub fn knows(&self, path: &str) -> bool {
        self.committed.contains_key(path) || self.uncommitted.contains(path)
    }
}
