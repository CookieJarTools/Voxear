//! Docs Health, as the CookieJarTools Cookbook has it: what needs looking
//! after in a project's pages.
//!
//! - **Out of date**: a page that links to a project file, such as a script
//!   or a level, changed on a later day than the page was. Both days are
//!   git's where git knows them (see [`crate::history`]), so a page edited
//!   anywhere counts; a page git does not know goes by the date in its
//!   header.
//! - **Links that go nowhere**, a page not written yet among them.
//! - **Missing pictures**: a picture a page shows that is not there.
//! - **Pictures no page shows**: files in the pages' own `images` folders
//!   that nothing points at any more. Pictures elsewhere in the project are
//!   left alone, since the game likely uses them.
//! - **Notes for code that is gone**: notes on Code Reference pages whose
//!   type was renamed, moved or taken out.
//!
//! It also lists **code with no page yet**, the project's own types with no
//! `[Documentation]`, and **comments still to write**, how many parts of
//! each of the project's types have no `///` comment, which are worth
//! knowing but not things gone wrong, so they are not counted.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::{
    library::{DOCS, Library, PageId, Target},
    markdown::{self, Block},
};

/// The extensions of the pictures VoxDocs shows.
const PICTURES: [&str; 4] = ["png", "jpg", "jpeg", "gif"];

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
    /// Notes whose type the code no longer has, by the names they are filed
    /// under.
    pub lost_notes: Vec<String>,
    /// The project's own types no page is about, as their Code Reference
    /// pages.
    pub undocumented: Vec<PageId>,
    /// Each of the project's types with parts that have no comment: its
    /// page, how many have none, and how many parts it has.
    pub comment_gaps: Vec<(PageId, usize, usize)>,
}

impl Health {
    /// Looks through every page.
    pub fn check(library: &Library) -> Self {
        let mut health = Health::default();
        let mut shown: HashSet<PathBuf> = HashSet::new();
        let mut linked: Vec<(PageId, Vec<String>)> = Vec::new();
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
            let mut files = Vec::new();
            for url in markdown::links(&blocks).into_iter().filter(|url| !pictures.contains(&url.as_str())) {
                match library.resolve(&url, id) {
                    Target::Missing(_) => health.broken.push((id, url)),
                    Target::File(path) if !page.generated => files.push(path),
                    _ => {}
                }
            }
            if !files.is_empty() {
                linked.push((id, files));
            }
        }
        health.stale = stale(library, linked);
        let docs = library.folder();
        let project = library.project();
        let mut unused: Vec<String> = picture_files(docs)
            .into_iter()
            .filter(|path| !shown.contains(path))
            .filter_map(|path| Some(path.strip_prefix(project).ok()?.to_string_lossy().replace('\\', "/")))
            .collect();
        unused.sort();
        health.unused_pictures = unused;
        health.lost_notes = library.lost_notes();
        health.undocumented = library.undocumented();
        health.comment_gaps = library.comment_gaps();
        health
    }

    /// How many things need looking after.
    pub fn problems(&self) -> usize {
        self.stale.len()
            + self.broken.len()
            + self.missing_pictures.len()
            + self.unused_pictures.len()
            + self.lost_notes.len()
    }
}

/// Each page that links to files changed on a later day than it was, with
/// those files. A page's day is git's when git knows the page, or else the
/// date in its header; one with neither is never out of date.
fn stale(library: &Library, linked: Vec<(PageId, Vec<String>)>) -> Vec<(PageId, Vec<String>)> {
    let page_file = |id: PageId| format!("{DOCS}/{}", library.page(id).file());
    let mut paths: Vec<String> =
        linked.iter().flat_map(|(id, files)| files.iter().cloned().chain([page_file(*id)])).collect();
    paths.sort();
    paths.dedup();
    let history = library.history(&paths);
    let project = library.project();
    let mut stale = Vec::new();
    for (id, mut files) in linked {
        let file = page_file(id);
        let updated = &library.page(id).header.updated;
        let day = if history.knows(&file) { history.changed(project, &file) } else { Some(updated.clone()) };
        let Some(day) = day.filter(|day| !day.is_empty()) else { continue };
        files.retain(|path| history.changed(project, path).is_some_and(|changed| changed > day));
        files.dedup();
        if !files.is_empty() {
            stale.push((id, files));
        }
    }
    stale
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
