//! Every page and folder in a project's `Docs` folder, as a tree, and the
//! ways to find and change them. A folder is a folder on disk; its header
//! and words come from a `_folder.md` inside it. A page is a `.md` file,
//! titled by its file name unless its header gives a title. Pages are plain
//! Markdown, so they can be written here or in any editor, and shared with
//! git like any other file.
//!
//! The [Code Reference](crate::code)'s pages come after them, in a folder of
//! their own, made from the code rather than read from files.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    code::{CODE_SCHEME, CodeReference},
    header::{self, Header},
    markdown::{self, PAGE_SCHEME, Task},
};

pub const EXTENSION: &str = ".md";
pub const FOLDER_FILE: &str = "_folder.md";

/// The Code Reference's folder, and its words.
const CODE_FOLDER: &str = "Code Reference";
const CODE_ABOUT: &str = "Every public type in the game's scripts and in Voxear.Core, with what its `///` comments \
say. These pages are made from the code each time the editor builds the scripts, so they are not written here: \
change the comments instead.

Any page can link to a type, as `[[code:Voxear.Prefab]]`, or `[[code:Voxear.Prefab|Prefab]]` to word the link \
itself.";
/// The path of the folder for types in the global namespace.
const GLOBAL_NAMESPACE: &str = "code:global::";

/// A page's place in its [`Library`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageId(pub usize);

#[derive(Clone, Debug)]
pub struct Page {
    /// Where it is in `Docs`, with forward slashes: `Combat/Damage.md` for a
    /// page, `Combat` for a folder, and nothing for the top folder.
    pub path: String,
    pub is_folder: bool,
    pub header: Header,
    pub body: String,
    pub parent: Option<PageId>,
    pub children: Vec<PageId>,
    /// Made from the code, as the Code Reference's pages are, rather than
    /// read from a file: it is read but not written. Its path is how links
    /// name it, `code:Voxear.Prefab`.
    pub generated: bool,
    /// Its title, tags and words in lower case, for searching.
    words: String,
}

impl Page {
    /// The file or folder's name, without `.md`.
    pub fn name(&self) -> &str {
        let name = self.path.rsplit('/').next().unwrap_or_default();
        name.strip_suffix(EXTENSION).unwrap_or(name)
    }

    pub fn title(&self) -> &str {
        if self.header.title.is_empty() { self.name() } else { &self.header.title }
    }

    /// How links and code name it: its folders and name, without `.md`.
    pub fn reference(&self) -> &str {
        self.path.strip_suffix(EXTENSION).unwrap_or(&self.path)
    }

    /// The file its header and Markdown are kept in, within `Docs`.
    pub fn file(&self) -> String {
        if self.is_folder { join(&self.path, FOLDER_FILE) } else { self.path.clone() }
    }

    /// The folder its relative links and pictures are found from.
    pub fn directory(&self) -> &str {
        if self.is_folder {
            return &self.path;
        }
        self.path.rsplit_once('/').map_or("", |(folder, _)| folder)
    }
}

/// Where a link in a page goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A page, and the heading on it, if any.
    Page(PageId, Option<String>),
    /// A heading on the page the link is in.
    Heading(String),
    /// A page that is not written yet, by the name the link gives it.
    Missing(String),
    Web(String),
    /// A file in the project, by its path from the project's folder.
    File(String),
}

pub struct Library {
    /// The `Docs` folder, and the project folder it is in.
    root: PathBuf,
    project: PathBuf,
    /// What the top folder is called when it has no title of its own.
    name: String,
    pages: Vec<Page>,
    /// How many pages are read from `Docs`; the Code Reference's come after.
    written: usize,
    code: CodeReference,
    by_id: HashMap<String, PageId>,
    by_reference: HashMap<String, PageId>,
    by_title: HashMap<String, Vec<PageId>>,
}

impl Library {
    /// The pages in `project/Docs`. A project with no `Docs` folder has an
    /// empty VoxDocs, whose first page makes the folder.
    pub fn open(project: &Path, name: &str) -> Self {
        let mut library = Self {
            root: project.join("Docs"),
            project: project.to_path_buf(),
            name: name.to_string(),
            pages: Vec::new(),
            written: 0,
            code: CodeReference::default(),
            by_id: HashMap::new(),
            by_reference: HashMap::new(),
            by_title: HashMap::new(),
        };
        library.reload();
        library
    }

    /// Reads every page again, as after they changed on disk.
    pub fn reload(&mut self) {
        self.pages.clear();
        self.by_id.clear();
        self.by_reference.clear();
        self.by_title.clear();
        let root = self.read_folder(String::new(), None);
        if self.pages[root.0].header.title.is_empty() {
            self.pages[root.0].header.title.clone_from(&self.name);
        }
        self.written = self.pages.len();
        self.add_code(root);
        for index in 0..self.pages.len() {
            self.index(PageId(index));
        }
    }

    pub fn root(&self) -> PageId {
        PageId(0)
    }

    pub fn page(&self, id: PageId) -> &Page {
        &self.pages[id.0]
    }

    pub fn pages(&self) -> impl Iterator<Item = (PageId, &Page)> {
        self.pages.iter().enumerate().map(|(i, p)| (PageId(i), p))
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Whether `Docs` has no pages, whatever the Code Reference has.
    pub fn is_empty(&self) -> bool {
        self.written <= 1
    }

    /// Shows the code as `code` describes it, in place of what it showed.
    /// The pages read from `Docs` keep their ids.
    pub fn set_code(&mut self, code: CodeReference) {
        self.code = code;
        self.reload();
    }

    /// The `Docs` folder.
    pub fn folder(&self) -> &Path {
        &self.root
    }

    /// The project's folder, which the `Docs` folder is in.
    pub fn project(&self) -> &Path {
        &self.project
    }

    /// A page by its id, its path (`Combat/Damage`, with or without `.md`)
    /// or, when only one page has it, its title.
    pub fn find(&self, reference: &str) -> Option<PageId> {
        let key = reference.trim().trim_matches('/');
        let key = key.strip_suffix(EXTENSION).unwrap_or(key);
        if key.is_empty() {
            return None;
        }
        self.by_id
            .get(key)
            .or_else(|| self.by_reference.get(key))
            .copied()
            .or_else(|| self.by_title.get(&key.to_lowercase()).filter(|found| found.len() == 1).map(|found| found[0]))
    }

    /// Where a link in page `from` goes: `[[Page]]` links by title, id or
    /// path; `#heading` on this page and `Page.md#heading` on another;
    /// paths relative to the page's folder; files in the project; and the web.
    pub fn resolve(&self, url: &str, from: PageId) -> Target {
        if let Some(anchor) = url.strip_prefix('#') {
            return Target::Heading(anchor.to_string());
        }
        if ["https://", "http://", "mailto:", "www."].iter().any(|scheme| url.starts_with(scheme)) {
            return Target::Web(url.to_string());
        }
        let (target, anchor) = match url.split_once('#') {
            Some((target, anchor)) => (target, Some(anchor.to_string())),
            None => (url, None),
        };
        let target = unescape(target);
        if let Some(name) = target.strip_prefix(PAGE_SCHEME) {
            return self
                .find(name)
                .map_or_else(|| Target::Missing(name.trim().to_string()), |page| Target::Page(page, anchor));
        }
        let relative = combine(self.page(from).directory(), &target);
        if let Some(page) = relative.as_deref().and_then(|path| self.find(path)).or_else(|| self.find(&target)) {
            return Target::Page(page, anchor);
        }
        if self.project.join(&target).exists() {
            return Target::File(target);
        }
        Target::Missing(target.strip_suffix(EXTENSION).unwrap_or(&target).to_string())
    }

    /// The file a picture in page `from` shows: a path from the page's
    /// folder, as pictures saved with a page are, or else from the project's
    /// folder. `None` for one not there, or on the web, which VoxDocs does
    /// not fetch.
    pub fn picture_path(&self, url: &str, from: PageId) -> Option<PathBuf> {
        if url.contains("://") {
            return None;
        }
        let url = unescape(url);
        let beside = combine(self.page(from).directory(), &url).map(|path| self.root.join(path));
        beside.filter(|path| path.is_file()).or_else(|| Some(self.project.join(&url)).filter(|path| path.is_file()))
    }

    /// Whether a link goes somewhere: false for a page not written yet.
    pub fn works(&self, url: &str, from: PageId) -> bool {
        !matches!(self.resolve(url, from), Target::Missing(_))
    }

    /// Every tag the pages use, with how many use it, the most used first.
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (_, page) in self.pages() {
            for tag in &page.header.tags {
                match counts.iter_mut().find(|(known, _)| known.eq_ignore_ascii_case(tag)) {
                    Some((_, count)) => *count += 1,
                    None => counts.push((tag.clone(), 1)),
                }
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        counts
    }

    /// Pages with every word of `query` in their title, tags or words,
    /// those with them all in the title first.
    pub fn search(&self, query: &str) -> Vec<PageId> {
        self.search_tagged(query, &[])
    }

    /// [`search`](Self::search), among the pages that have every one of
    /// `tags`. With tags and no words, every page that has them.
    pub fn search_tagged(&self, query: &str, tags: &[String]) -> Vec<PageId> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() && tags.is_empty() {
            return Vec::new();
        }
        let tagged =
            |page: &Page| tags.iter().all(|tag| page.header.tags.iter().any(|has| has.eq_ignore_ascii_case(tag)));
        let mut found: Vec<(bool, PageId)> = self
            .pages()
            .filter(|(_, page)| tagged(page) && words.iter().all(|w| page.words.contains(w.as_str())))
            .map(|(id, page)| (words.iter().all(|w| page.title().to_lowercase().contains(w.as_str())), id))
            .collect();
        // The project's own pages before the Code Reference's.
        found.sort_by(|a, b| {
            let (page_a, page_b) = (self.page(a.1), self.page(b.1));
            b.0.cmp(&a.0).then(page_a.generated.cmp(&page_b.generated)).then_with(|| page_a.title().cmp(page_b.title()))
        });
        found.into_iter().map(|(_, id)| id).collect()
    }

    /// Writes a page's Markdown, giving it an id if it has none and today's
    /// date as when it was updated.
    pub fn save(&mut self, id: PageId, body: &str) -> Result<()> {
        if self.page(id).generated {
            bail!("{} is made from the code, so it is not written here", self.page(id).title());
        }
        let page = &mut self.pages[id.0];
        if page.header.id.is_empty() {
            page.header.id = header::new_id();
        }
        page.header.updated = header::today();
        page.body = body.to_string();
        let file = self.root.join(page.file());
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
        }
        let text = page.header.join(body);
        std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
        self.index(id);
        Ok(())
    }

    /// Writes a page as its editor left it: its title, tags and Markdown. A
    /// new title renames its file, or a folder's folder, unless its header
    /// holds the title, which then changes there; the top folder keeps its
    /// name. Returns the page as it now is, since a rename moves it.
    pub fn save_page(&mut self, id: PageId, title: &str, tags: &[String], body: &str) -> Result<PageId> {
        let mut kept: Vec<String> = Vec::new();
        for tag in tags.iter().map(|tag| tag.trim()).filter(|tag| !tag.is_empty()) {
            if !kept.iter().any(|k| k.eq_ignore_ascii_case(tag)) {
                kept.push(tag.to_string());
            }
        }
        self.pages[id.0].header.tags = kept;
        let title = title.trim().replace(['/', '\\'], "-");
        let page = self.page(id);
        if title.is_empty() || id == self.root() || title == page.title() {
            self.save(id, body)?;
            return Ok(id);
        }
        if !page.header.title.is_empty() {
            self.pages[id.0].header.title = title;
            self.save(id, body)?;
            return Ok(id);
        }
        let name = if page.is_folder { title.clone() } else { format!("{title}{EXTENSION}") };
        let parent = page.parent.map_or(String::new(), |parent| self.page(parent).path.clone());
        let (from, to) = (self.root.join(&page.path), self.root.join(join(&parent, &name)));
        if to.exists() && !same_file(&from, &to) {
            bail!("there is already a page called {title} in this folder");
        }
        self.save(id, body)?;
        let saved = self.page(id).header.id.clone();
        std::fs::rename(&from, &to).with_context(|| format!("renaming {} to {title}", self.page(id).title()))?;
        self.reload();
        self.by_id.get(&saved).copied().context("the page was not found after renaming it")
    }

    /// Ticks or unticks a checklist box, and saves the page.
    pub fn set_task(&mut self, id: PageId, task: Task, done: bool) -> Result<()> {
        let mut body = self.page(id).body.clone();
        let mark = if done { "x" } else { " " };
        if !matches!(body.get(task.at..task.at + 1), Some(" " | "x" | "X")) {
            bail!("the checklist in {} changed since it was shown", self.page(id).title());
        }
        body.replace_range(task.at..task.at + 1, mark);
        self.save(id, &body)
    }

    /// A new page called `name` in `folder`, written straight away so it
    /// is there for the next link to it. A page of that name already there
    /// is found rather than replaced.
    pub fn create(&mut self, folder: PageId, name: &str) -> Result<PageId> {
        let name = name.trim().replace(['/', '\\'], "-");
        if name.is_empty() {
            bail!("a page needs a name");
        }
        let folder = match self.page(folder) {
            page if page.generated => self.root(),
            page if page.is_folder => folder,
            page => page.parent.unwrap_or(self.root()),
        };
        let path = join(&self.page(folder).path, &format!("{name}{EXTENSION}"));
        if let Some(found) = self.by_reference.get(path.strip_suffix(EXTENSION).unwrap_or(&path)) {
            return Ok(*found);
        }
        let mut header = Header::default();
        (header.id, header.updated) = (header::new_id(), header::today());
        let file = self.root.join(&path);
        std::fs::create_dir_all(file.parent().unwrap_or(&self.root))?;
        std::fs::write(&file, header.join("")).with_context(|| format!("writing {}", file.display()))?;
        self.reload();
        self.find(&path).context("the new page was not found after writing it")
    }

    fn read_folder(&mut self, path: String, parent: Option<PageId>) -> PageId {
        let disk = self.root.join(&path);
        let (header, body) =
            std::fs::read_to_string(disk.join(FOLDER_FILE)).map(|t| Header::split(&t)).unwrap_or_default();
        let id = self.add(Page {
            path: path.clone(),
            is_folder: true,
            header,
            body,
            parent,
            children: Vec::new(),
            generated: false,
            words: String::new(),
        });
        let mut entries: Vec<(String, bool)> = std::fs::read_dir(&disk)
            .map(|dir| {
                dir.flatten()
                    .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().is_dir()))
                    .filter(|(name, _)| !name.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        entries.sort();
        for (name, is_dir) in entries {
            let child_path = join(&path, &name);
            let child = if is_dir {
                // Pictures live in `images` folders beside their pages.
                if name == "images" {
                    continue;
                }
                self.read_folder(child_path, Some(id))
            } else if name.ends_with(EXTENSION) && name != FOLDER_FILE {
                let text = std::fs::read_to_string(disk.join(&name)).unwrap_or_default();
                let (header, body) = Header::split(&text);
                let page = Page {
                    path: child_path,
                    is_folder: false,
                    header,
                    body,
                    parent: Some(id),
                    children: Vec::new(),
                    generated: false,
                    words: String::new(),
                };
                self.add(page)
            } else {
                continue;
            };
            self.pages[id.0].children.push(child);
        }
        let mut children = std::mem::take(&mut self.pages[id.0].children);
        children.sort_by(|a, b| {
            let (a, b) = (&self.pages[a.0], &self.pages[b.0]);
            a.header
                .order
                .cmp(&b.header.order)
                .then(b.is_folder.cmp(&a.is_folder))
                .then_with(|| a.title().to_lowercase().cmp(&b.title().to_lowercase()))
        });
        self.pages[id.0].children = children;
        id
    }

    fn add(&mut self, page: Page) -> PageId {
        self.pages.push(page);
        PageId(self.pages.len() - 1)
    }

    /// The Code Reference's folder, last in the top folder: a folder for
    /// each namespace, and a page for each type in it.
    fn add_code(&mut self, root: PageId) {
        if self.code.types().is_empty() {
            return;
        }
        let code = std::mem::take(&mut self.code);
        let top = self.add_generated(root, CODE_SCHEME.to_string(), CODE_FOLDER, CODE_ABOUT.to_string(), true);
        for namespace in code.namespaces() {
            let (path, title) = match namespace {
                "" => (GLOBAL_NAMESPACE.to_string(), "Global namespace"),
                namespace => (format!("{CODE_SCHEME}{namespace}"), namespace),
            };
            let folder = self.add_generated(top, path, title, String::new(), true);
            for kind in code.types().iter().filter(|t| t.namespace == namespace) {
                self.add_generated(folder, format!("{CODE_SCHEME}{}", kind.key), &kind.title(), kind.page(), false);
            }
        }
        self.code = code;
    }

    fn add_generated(&mut self, parent: PageId, path: String, title: &str, body: String, is_folder: bool) -> PageId {
        let mut header = Header::default();
        header.title = title.to_string();
        let page = Page {
            path,
            is_folder,
            header,
            body,
            parent: Some(parent),
            children: Vec::new(),
            generated: true,
            words: String::new(),
        };
        let id = self.add(page);
        self.pages[parent.0].children.push(id);
        id
    }

    /// Files a page under its id, path and title, and notes its words.
    fn index(&mut self, id: PageId) {
        let page = &self.pages[id.0];
        let words = format!(
            "{} {} {}",
            page.title(),
            page.header.tags.join(" "),
            markdown::plain_text(&markdown::parse(&page.body))
        )
        .to_lowercase();
        if !page.header.id.is_empty() {
            self.by_id.insert(page.header.id.clone(), id);
        }
        // The Code Reference's pages are found by their paths alone, so a page
        // named as a type is still found by its title.
        if !page.path.is_empty() {
            self.by_reference.insert(page.reference().to_string(), id);
        }
        if !page.path.is_empty() && !page.generated {
            let titles = self.by_title.entry(page.title().to_lowercase()).or_default();
            if !titles.contains(&id) {
                titles.push(id);
            }
        }
        self.pages[id.0].words = words;
    }
}

/// Whether two paths are one file, as a rename that only changes the case of
/// a name finds on Windows.
fn same_file(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

/// A path relative to `folder`, made into one from `Docs`, or `None` if it
/// climbs out.
fn combine(folder: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// `%20` and the like, as links write spaces.
fn unescape(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text.get(i + 1..i + 3).and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}
