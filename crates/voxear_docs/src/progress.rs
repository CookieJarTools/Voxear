//! How far a person has got through each guide: the action steps they have
//! run, kept for this project on their machine, through restarts, in
//! `.voxear/VoxDocs.toml`. The `.voxear` folder is each person's own, not
//! shared with git, so everyone works through a guide themselves. Checks
//! are not kept: they look at the project again each time.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const FILE: &str = "VoxDocs.toml";

#[derive(Debug, Default)]
pub struct Progress {
    file: PathBuf,
    saved: Saved,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Saved {
    /// The steps done on each page, by the page's id, or its path when it has none.
    #[serde(default)]
    steps: BTreeMap<String, BTreeSet<String>>,
}

impl Progress {
    /// The progress kept in the project in `project`, or none yet.
    pub fn open(project: &Path) -> Self {
        let file = project.join(".voxear").join(FILE);
        let saved = std::fs::read_to_string(&file).ok().and_then(|text| toml::from_str(&text).ok()).unwrap_or_default();
        Self { file, saved }
    }

    pub fn is_done(&self, page: &str, step: &str) -> bool {
        self.saved.steps.get(page).is_some_and(|done| done.contains(step))
    }

    pub fn set_done(&mut self, page: &str, step: &str) {
        if self.saved.steps.entry(page.to_string()).or_default().insert(step.to_string()) {
            self.save();
        }
    }

    /// Starts a page's guide again.
    pub fn reset(&mut self, page: &str) {
        if self.saved.steps.remove(page).is_some() {
            self.save();
        }
    }

    fn save(&self) {
        let written = toml::to_string(&self.saved).map_err(anyhow::Error::from).and_then(|text| {
            if let Some(folder) = self.file.parent() {
                std::fs::create_dir_all(folder)?;
            }
            Ok(std::fs::write(&self.file, text)?)
        });
        if let Err(error) = written {
            tracing::error!("VoxDocs: could not keep how far the guides have got: {error:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_kept_until_the_guide_starts_again() {
        let dir = tempfile::tempdir().unwrap();
        let mut progress = Progress::open(dir.path());
        progress.set_done("a4f08c3e91b2", "action:Play");
        let again = Progress::open(dir.path());
        assert!(again.is_done("a4f08c3e91b2", "action:Play") && !again.is_done("a4f08c3e91b2", "action:Save"));
        progress.reset("a4f08c3e91b2");
        assert!(!Progress::open(dir.path()).is_done("a4f08c3e91b2", "action:Play"));
    }
}
