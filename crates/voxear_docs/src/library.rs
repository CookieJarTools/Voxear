//! Every page and folder in a project's `Docs` folder, as a tree, and the
//! ways to find and change them. A folder is a folder on disk; its header
//! and words come from a `_folder.md` inside it. A page is a `.md` file,
//! titled by its file name unless its header gives a title. Pages are plain
//! Markdown, so they can be written here or in any editor, and shared with
//! git like any other file.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    header::{self, Header},
    markdown::{self, PAGE_SCHEME, Task},
};

pub const EXTENSION: &str = ".md";
pub const FOLDER_FILE: &str = "_folder.md";

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

    pub fn is_empty(&self) -> bool {
        self.pages.len() <= 1
    }

    /// The `Docs` folder.
    pub fn folder(&self) -> &Path {
        &self.root
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

    /// Whether a link goes somewhere: false for a page not written yet.
    pub fn works(&self, url: &str, from: PageId) -> bool {
        !matches!(self.resolve(url, from), Target::Missing(_))
    }

    /// Pages with every word of `query` in their title, tags or words,
    /// those with them all in the title first.
    pub fn search(&self, query: &str) -> Vec<PageId> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<(bool, PageId)> = self
            .pages()
            .filter(|(_, page)| words.iter().all(|w| page.words.contains(w.as_str())))
            .map(|(id, page)| (words.iter().all(|w| page.title().to_lowercase().contains(w.as_str())), id))
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| self.page(a.1).title().cmp(self.page(b.1).title())));
        found.into_iter().map(|(_, id)| id).collect()
    }

    /// Writes a page's Markdown, giving it an id if it has none and today's
    /// date as when it was updated.
    pub fn save(&mut self, id: PageId, body: &str) -> Result<()> {
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
        let folder = if self.page(folder).is_folder { folder } else { self.page(folder).parent.unwrap_or(self.root()) };
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
        if !page.path.is_empty() {
            self.by_reference.insert(page.reference().to_string(), id);
            let titles = self.by_title.entry(page.title().to_lowercase()).or_default();
            if !titles.contains(&id) {
                titles.push(id);
            }
        }
        self.pages[id.0].words = words;
    }
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
