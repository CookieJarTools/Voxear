//! Docs Health, as the CookieJarTools Cookbook has it: what needs looking
//! after in a project's pages.
//!
//! - **Out of date**: a page that links to a project file, such as a script
//!   or a level, changed on a later day than the page was last updated.
//! - **Links that go nowhere**, a page not written yet among them.
//! - **Missing pictures**: a picture a page shows that is not there.
//! - **Pictures no page shows**: files in the pages' own `images` folders
//!   that nothing points at any more. Pictures elsewhere in the project are
//!   left alone, since the game likely uses them.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::{
    header,
    library::{Library, PageId, Target},
    markdown::{self, Block},
};

/// The extensions of the pictures VoxDocs shows.
const PICTURES: [&str; 3] = ["png", "jpg", "jpeg"];

#[derive(Debug, Default)]
pub struct Health {
    /// Each page out of date, with the project files it links to that changed
    /// since, by their paths in the project.
    pub stale: Vec<(PageId, Vec<String>)>,
    /// Each link that goes nowhere: its page, and its address.
    pub broken: Vec<(PageId, String)>,
    /// Each picture a page shows that is not there: its page, and its address.
    pub missing_pictures: Vec<(PageId, String)>,
    /// Pictures in the pages' `images` folders that no page shows, by their
    /// paths in the project.
    pub unused_pictures: Vec<String>,
}

impl Health {
    /// Looks through every page.
    pub fn check(library: &Library) -> Self {
        let mut health = Health::default();
        let mut shown: HashSet<PathBuf> = HashSet::new();
        for (id, page) in library.pages() {
            let blocks = markdown::parse(&page.body);
            let pictures: Vec<&str> = blocks
                .iter()
                .filter_map(|block| match block {
                    Block::Image { url, .. } => Some(url.as_str()),
                    _ => None,
                })
                .collect();
            for url in &pictures {
                match library.picture_path(url, id) {
                    Some(path) => drop(shown.insert(path)),
                    None if !url.contains("://") => health.missing_pictures.push((id, url.to_string())),
                    None => {}
                }
            }
            let mut changed = Vec::new();
            for url in markdown::links(&blocks).into_iter().filter(|url| !pictures.contains(&url.as_str())) {
                match library.resolve(&url, id) {
                    Target::Missing(_) => health.broken.push((id, url)),
                    Target::File(path) if newer(library.project(), &path, &page.header.updated) => changed.push(path),
                    _ => {}
                }
            }
            if !changed.is_empty() {
                changed.dedup();
                health.stale.push((id, changed));
            }
        }
        let docs = library.folder();
        let project = library.project();
        let mut unused: Vec<String> = picture_files(docs)
            .into_iter()
            .filter(|path| !shown.contains(path))
            .filter_map(|path| Some(path.strip_prefix(project).ok()?.to_string_lossy().replace('\\', "/")))
            .collect();
        unused.sort();
        health.unused_pictures = unused;
        health
    }

    /// How many things need looking after.
    pub fn problems(&self) -> usize {
        self.stale.len() + self.broken.len() + self.missing_pictures.len() + self.unused_pictures.len()
    }
}

/// Whether a project file changed on a later day than `updated`, a page's
/// `2026-10-31`. A page with no date is never out of date.
fn newer(project: &Path, file: &str, updated: &str) -> bool {
    if updated.is_empty() {
        return false;
    }
    let modified = std::fs::metadata(project.join(file)).and_then(|m| m.modified());
    modified.is_ok_and(|time| header::date_of(time).as_str() > updated)
}

/// Every picture file in an `images` folder under `folder`.
fn picture_files(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(picture_files(&path));
            continue;
        }
        let in_images = folder.file_name().is_some_and(|name| name == "images");
        let picture = path.extension().is_some_and(|e| PICTURES.contains(&e.to_string_lossy().to_lowercase().as_str()));
        if in_images && picture {
            found.push(path);
        }
    }
    found
}
