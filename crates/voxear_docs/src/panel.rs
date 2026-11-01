//! The VoxDocs panel: the pages down the side and the page beside them,
//! as in the CookieJarTools Cookbook. Click a page to read it; links go to
//! other pages, headings, files and the web, and a link to a page not
//! written yet makes it. Ticking a checklist box saves the page. Back and
//! forward go through the pages read, and the box at the top finds pages by
//! their words. Tags over the tree filter the pages to those that have every
//! one picked, and a tag on a page picks it too. A long page lists its
//! headings beside it, "On this page", when there is room, picking out the
//! one being read.
//!
//! **Edit** writes the page: its title and tags, its Markdown, and the page
//! as it will read beside it, drawn again a moment after each change. Save
//! or Ctrl+S writes it to its file, and so does Done, or opening another
//! page, so nothing written is lost. A new title renames the page's file.
//!
//! The top page is home, as the Cookbook's is: under its own words and what
//! is in it, the pages updated most lately, and how healthy the docs are.
//!
//! **Health** shows what needs looking after (see [`crate::health`]) in the
//! page's place, each entry a link to the page or file it is about.
//!
//! The **Code Reference** comes last in the tree once the editor hands it
//! over with [`set_code`]. Its pages are read only, so they have no Edit.
//!
//! **Pictures** are files beside a page, or anywhere in the project. The
//! panel cannot read them into textures itself: [`wanted_pictures`] lists
//! the files a page needs, and [`set_picture`] hands each back once the
//! editor has loaded it, which draws the page again.
//!
//! [`wanted_pictures`]: VoxDocs::wanted_pictures
//! [`set_picture`]: VoxDocs::set_picture
//! [`set_code`]: VoxDocs::set_code

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

use voxear_ui::{ImageRef, ItemFns, NodeId, TreeItems, Ui};

use crate::{
    code::{CODE_SCHEME, CodeReference},
    health::Health,
    library::{Library, Page, PageId, Target},
    markdown::{self, PAGE_SCHEME},
    view::{self, Lookups, Picture, Rendered},
};

const MARKUP: &str = include_str!("../ui/VoxDocs.vui");
const SHEET: &str = include_str!("../ui/VoxDocs.vss");

/// How many pages home lists as updated lately.
const RECENT: usize = 6;
/// How many tags show before Show all, beside any picked.
const TAGS_AT_FIRST: usize = 8;
/// "On this page" shows for a page with at least this many headings, when
/// the reading area is at least this wide.
const TOC_FEWEST: usize = 3;
const TOC_ROOM: f32 = 640.0;
/// How far down the page a heading counts as the one being read.
const READING_LINE: f32 = 60.0;

/// How long after a change the preview is drawn again, so typing stays quick.
const PREVIEW_DELAY: Duration = Duration::from_millis(250);

/// What an empty VoxDocs says, as Markdown, so its link makes the first page.
const EMPTY: &str = "This project has no pages yet. Pages are Markdown files in its `Docs` folder, \
                     so they can be written here or in any editor and shared with git.\n\n\
                     Start with [[Welcome]], or link to a page that does not exist yet and click the link to write it.";

type Shared = Rc<RefCell<Library>>;

/// The page tree: the top folder, and in each folder its folders and pages.
struct PageTree(Shared);

impl TreeItems for PageTree {
    fn children(&self, parent: Option<u64>) -> Vec<u64> {
        match parent {
            None => vec![0],
            Some(id) => self.0.borrow().page(PageId(id as usize)).children.iter().map(|c| c.0 as u64).collect(),
        }
    }

    fn has_children(&self, item: u64) -> bool {
        !self.0.borrow().page(PageId(item as usize)).children.is_empty()
    }

    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        let icon = ui.icon(row, "file");
        ui.add_class(icon, "cb-row-icon");
        let title = ui.add_label(row, "");
        ui.add_class(title, "cb-row-title");
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let library = self.0.borrow();
        let page = library.page(PageId(item as usize));
        let parts = ui.children(row);
        ui.set_icon(parts[0], page_icon(page));
        ui.set_text(parts[1], page.title());
    }
}

/// A page's icon in lists: a folder, a page, or a type of the Code Reference.
fn page_icon(page: &Page) -> &'static str {
    match page {
        page if page.is_folder => "folder",
        page if page.generated => "code",
        _ => "file",
    }
}

/// Elements the panel reads and writes, by name in VoxDocs.vui.
struct Nodes {
    tree: NodeId,
    results: NodeId,
    search: NodeId,
    back: NodeId,
    forward: NodeId,
    path: NodeId,
    tags: NodeId,
    tag_chips: NodeId,
    tags_clear: NodeId,
    tags_more: NodeId,
    reading: NodeId,
    toc: NodeId,
    health: NodeId,
    edit: NodeId,
    edit_icon: NodeId,
    edit_label: NodeId,
    page: NodeId,
    editor: NodeId,
    title: NodeId,
    tags_field: NodeId,
    status: NodeId,
    save: NodeId,
    source: NodeId,
    preview: NodeId,
}

pub struct VoxDocs {
    library: Shared,
    nodes: Nodes,
    open: PageId,
    /// The pages read before and after this one, by id or path, which stay
    /// true when pages are added or renamed.
    back: Vec<String>,
    forward: Vec<String>,
    rendered: Rendered,
    editing: bool,
    /// Something written has not been saved.
    dirty: bool,
    /// The Markdown takes focus once it shows, a frame after Edit.
    focus_source: bool,
    /// When the preview is to be drawn again.
    preview_due: Option<Instant>,
    /// Each picture file asked for, and how it shows; and the ones drawn as
    /// loading that are still to be asked for.
    pictures: HashMap<PathBuf, Picture>,
    wanted: RefCell<Vec<PathBuf>>,
    /// A picture came in, so the page is drawn again.
    redraw: bool,
    /// The tags filtering the pages, whether every tag shows, and the chips
    /// shown, over the tree and on the page.
    picked_tags: Vec<String>,
    all_tags: bool,
    tag_chips: Vec<(NodeId, String)>,
    page_tags: Vec<(NodeId, String)>,
    /// "On this page": each heading's link, and the heading it goes to.
    toc: Vec<(NodeId, NodeId, String)>,
    /// Docs Health shows in the page's place, and home's line that opens it.
    showing_health: bool,
    home_health: Option<NodeId>,
    /// A heading to scroll to once the page has been laid out.
    pending_anchor: Option<String>,
    results: Rc<RefCell<Vec<PageId>>>,
    /// A file in the project a link asked to show, for the editor.
    file_request: Option<String>,
}

impl VoxDocs {
    /// Builds the panel in `parent` for the pages in `project/Docs`; `name`
    /// titles the top folder, as the project's name does.
    pub fn new(ui: &mut Ui, parent: NodeId, project: &std::path::Path, name: &str) -> Self {
        let mut load = |path: &str| match path {
            "VoxDocs.vss" => Ok(SHEET.to_string()),
            other => Err(format!("{other} is not built in")),
        };
        let document =
            ui.load_markup(parent, "VoxDocs.vui", MARKUP, &mut load).expect("the VoxDocs markup is built in");
        let find = |ui: &Ui, name: &str| ui.find(document, name).unwrap_or_else(|| panic!("VoxDocs.vui has no {name}"));
        let nodes = Nodes {
            tree: find(ui, "cb-tree"),
            results: find(ui, "cb-results"),
            search: find(ui, "cb-search"),
            back: find(ui, "cb-back"),
            forward: find(ui, "cb-forward"),
            path: find(ui, "cb-path"),
            tags: find(ui, "cb-tags"),
            tag_chips: find(ui, "cb-tag-chips"),
            tags_clear: find(ui, "cb-tags-clear"),
            tags_more: find(ui, "cb-tags-more"),
            reading: find(ui, "cb-reading"),
            toc: find(ui, "cb-toc"),
            health: find(ui, "cb-health"),
            edit: find(ui, "cb-edit"),
            edit_icon: find(ui, "cb-edit-icon"),
            edit_label: find(ui, "cb-edit-label"),
            page: find(ui, "cb-page"),
            editor: find(ui, "cb-editor"),
            title: find(ui, "cb-title-field"),
            tags_field: find(ui, "cb-tags-field"),
            status: find(ui, "cb-status"),
            save: find(ui, "cb-save"),
            source: find(ui, "cb-source"),
            preview: find(ui, "cb-preview"),
        };
        let library = Rc::new(RefCell::new(Library::open(project, name)));
        ui.bind_tree(nodes.tree, PageTree(library.clone()));
        ui.set_expanded(nodes.tree, 0, true);
        let results: Rc<RefCell<Vec<PageId>>> = Rc::default();
        let (shown, titles) = (results.clone(), library.clone());
        let rows = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                let icon = ui.icon(row, "file");
                ui.add_class(icon, "cb-row-icon");
                let title = ui.add_label(row, "");
                ui.add_class(title, "cb-row-title");
                let folder = ui.add_label(row, "");
                ui.add_class(folder, "cb-row-folder");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let Some(&id) = shown.borrow().get(index) else { return };
                let library = titles.borrow();
                let page = library.page(id);
                let parts = ui.children(row);
                ui.set_icon(parts[0], page_icon(page));
                ui.set_text(parts[1], page.title());
                let folder = page.parent.map(|p| library.page(p).title().to_string()).unwrap_or_default();
                ui.set_text(parts[2], &folder);
            },
        };
        ui.bind_list(nodes.results, 0, rows);
        ui.set_visible(nodes.results, false);
        let mut docs = Self {
            library,
            nodes,
            open: PageId(0),
            back: Vec::new(),
            forward: Vec::new(),
            rendered: Rendered::default(),
            editing: false,
            dirty: false,
            focus_source: false,
            preview_due: None,
            pictures: HashMap::new(),
            wanted: RefCell::default(),
            redraw: false,
            picked_tags: Vec::new(),
            all_tags: false,
            tag_chips: Vec::new(),
            page_tags: Vec::new(),
            toc: Vec::new(),
            showing_health: false,
            home_health: None,
            pending_anchor: None,
            results,
            file_request: None,
        };
        docs.show_editing(ui, false);
        docs.render(ui);
        docs.show_tags(ui);
        docs
    }

    /// The pages, to read or find one.
    pub fn library(&self) -> std::cell::Ref<'_, Library> {
        self.library.borrow()
    }

    /// The page being read.
    pub fn open_page(&self) -> PageId {
        self.open
    }

    /// Whether the page is being written.
    pub fn is_editing(&self) -> bool {
        self.editing
    }

    /// Whether the page is being written with the keyboard in it, so Ctrl+S
    /// is for the page rather than the scene.
    pub fn has_keyboard(&self, ui: &Ui) -> bool {
        let fields = [self.nodes.title, self.nodes.tags_field, self.nodes.source];
        self.editing && ui.focused().is_some_and(|focused| fields.contains(&focused))
    }

    /// Up to `limit` picture files the page shows that are not read yet, for
    /// the editor to load and hand back with [`set_picture`](Self::set_picture).
    pub fn wanted_pictures(&mut self, limit: usize) -> Vec<PathBuf> {
        let mut wanted = self.wanted.borrow_mut();
        let count = limit.min(wanted.len());
        let taken: Vec<PathBuf> = wanted.drain(..count).collect();
        drop(wanted);
        for path in &taken {
            self.pictures.insert(path.clone(), Picture::Loading);
        }
        taken
    }

    /// A picture file read into a texture, with its size in pixels, or
    /// `None` when it could not be. The page showing it is drawn again.
    pub fn set_picture(&mut self, path: &Path, picture: Option<(ImageRef, [u32; 2])>) {
        let shown = picture.map_or(Picture::Missing, |(image, size)| Picture::Shown(image, size));
        self.pictures.insert(path.to_path_buf(), shown);
        self.redraw = true;
    }

    /// How a picture in the open page shows: once read, or else as loading,
    /// noting it to be read, or as missing.
    fn picture(&self, library: &Library, url: &str) -> Picture {
        let Some(path) = library.picture_path(url, self.open) else { return Picture::Missing };
        if let Some(picture) = self.pictures.get(&path) {
            return *picture;
        }
        let mut wanted = self.wanted.borrow_mut();
        if !wanted.contains(&path) {
            wanted.push(path);
        }
        Picture::Loading
    }

    /// Saves the page being written, as Save and Ctrl+S do.
    pub fn save(&mut self, ui: &mut Ui) {
        if !self.editing || !self.dirty {
            return;
        }
        let title = ui.text(self.nodes.title);
        let tags: Vec<String> = ui.text(self.nodes.tags_field).split(',').map(str::to_string).collect();
        let body = ui.text(self.nodes.source);
        let saved = self.library.borrow_mut().save_page(self.open, &title, &tags, &body);
        match saved {
            Ok(page) => {
                self.open = page;
                self.dirty = false;
                ui.refresh_list(self.nodes.tree);
                self.select_in_tree(ui);
                self.show_path(ui);
                self.show_tags(ui);
            }
            Err(error) => tracing::error!("VoxDocs: could not save the page: {error:#}"),
        }
        self.show_status(ui);
    }

    /// Opens a page by its id, path or title, as code's `[Documentation]`
    /// links will. Returns whether there is such a page.
    pub fn open_reference(&mut self, ui: &mut Ui, reference: &str) -> bool {
        let (target, anchor) = match reference.split_once('#') {
            Some((target, anchor)) => (target, Some(anchor.to_string())),
            None => (reference, None),
        };
        let found = self.library.borrow().find(target);
        if let Some(page) = found {
            self.go(ui, page, anchor, true);
        }
        found.is_some()
    }

    /// Shows the Code Reference as `code` describes it, as the scripts were
    /// last built. A page of it being read stays open if its type is still
    /// there.
    pub fn set_code(&mut self, ui: &mut Ui, code: CodeReference) {
        let open = self.key(self.open);
        self.library.borrow_mut().set_code(code);
        let found = self.library.borrow().find(&open);
        let gone = found.is_none();
        self.open = found.unwrap_or(PageId(0));
        ui.refresh_list(self.nodes.tree);
        self.search(ui);
        if !self.editing && !self.showing_health {
            self.render(ui);
        }
        if gone {
            self.select_in_tree(ui);
        }
    }

    /// A file in the project that a link asked to show, once.
    pub fn take_file_request(&mut self) -> Option<String> {
        self.file_request.take()
    }

    /// Answers what was clicked and typed since the last frame.
    pub fn update(&mut self, ui: &mut Ui) {
        self.scroll_to_pending(ui);
        self.update_writing(ui);
        if std::mem::take(&mut self.redraw) {
            if self.editing {
                self.render_preview(ui);
            } else {
                self.render(ui);
            }
        }
        let nodes = &self.nodes;
        if ui.selection_changed(nodes.tree)
            && let Some(&id) = ui.selected_items(nodes.tree).first()
        {
            self.go(ui, PageId(id as usize), None, true);
        }
        let picked = ui.selection(self.nodes.results).first().and_then(|&i| self.results.borrow().get(i).copied());
        if ui.selection_changed(self.nodes.results)
            && let Some(id) = picked
        {
            self.go(ui, id, None, true);
        }
        if ui.changed(self.nodes.search) {
            self.search(ui);
        }
        self.update_tags(ui);
        self.update_toc(ui);
        if ui.clicked(self.nodes.back) {
            self.step_history(ui, true);
        }
        if ui.clicked(self.nodes.forward) {
            self.step_history(ui, false);
        }
        if let Some(url) = ui.clicked_link(self.nodes.page).map(str::to_string) {
            self.follow(ui, &url);
        }
        if !self.editing {
            self.tick_tasks(ui);
        }
        let copied = self.rendered.copies.iter().find(|(button, _)| ui.clicked(*button)).map(|(_, code)| code.clone());
        if let Some(code) = copied {
            ui.copy_text(&code);
        }
        if self.home_health.is_some_and(|line| ui.clicked(line)) {
            self.show_health(ui);
        }
        if ui.clicked(self.nodes.health) {
            self.finish_editing(ui);
            if self.showing_health {
                self.render(ui);
            } else {
                self.show_health(ui);
            }
        }
        if ui.clicked(self.nodes.edit) {
            if self.editing {
                self.finish_editing(ui);
            } else {
                self.start_editing(ui);
            }
        }
        if ui.clicked(self.nodes.save) {
            self.save(ui);
        }
    }

    /// While writing: the Markdown takes focus once it shows, a change marks
    /// the page unsaved, and the preview follows a moment later.
    fn update_writing(&mut self, ui: &mut Ui) {
        if !self.editing {
            return;
        }
        if self.focus_source {
            ui.focus(self.nodes.source);
            if ui.focused() == Some(self.nodes.source) {
                ui.caret_to_end(self.nodes.source);
                self.focus_source = false;
            }
        }
        if ui.changed(self.nodes.source) {
            self.preview_due = Some(Instant::now() + PREVIEW_DELAY);
        }
        if [self.nodes.source, self.nodes.title, self.nodes.tags_field].iter().any(|&field| ui.changed(field))
            && !self.dirty
        {
            self.dirty = true;
            self.show_status(ui);
        }
        if self.preview_due.is_some_and(|due| Instant::now() >= due) {
            self.preview_due = None;
            self.render_preview(ui);
        }
    }

    /// Writes the open page: its title, tags and Markdown in fields, and how
    /// it reads beside them.
    fn start_editing(&mut self, ui: &mut Ui) {
        let library = self.library.borrow();
        let page = library.page(self.open);
        let is_root = self.open == library.root();
        ui.set_text(self.nodes.title, if is_root { "" } else { page.title() });
        ui.set_enabled(self.nodes.title, !is_root);
        ui.set_text(self.nodes.tags_field, &page.header.tags.join(", "));
        ui.set_text(self.nodes.source, &page.body);
        drop(library);
        self.dirty = false;
        self.show_editing(ui, true);
        self.show_status(ui);
        self.render_preview(ui);
        self.focus_source = true;
    }

    /// Stops writing, saving what was written, and shows the page.
    fn finish_editing(&mut self, ui: &mut Ui) {
        if !self.editing {
            return;
        }
        self.save(ui);
        self.show_editing(ui, false);
        self.render(ui);
    }

    /// How a page is remembered for Back and Forward: its id, or its path.
    fn key(&self, page: PageId) -> String {
        let library = self.library.borrow();
        let page = library.page(page);
        if page.header.id.is_empty() { page.reference().to_string() } else { page.header.id.clone() }
    }

    /// Opens a page, remembering the one left for Back when `record` is set.
    /// A page being written is saved first.
    fn go(&mut self, ui: &mut Ui, page: PageId, anchor: Option<String>, record: bool) {
        if page != self.open {
            self.finish_editing(ui);
            if record {
                self.back.push(self.key(self.open));
                self.forward.clear();
            }
            self.open = page;
            self.render(ui);
            ui.set_scroll(self.nodes.page, [0.0, 0.0]);
            self.select_in_tree(ui);
        }
        self.pending_anchor = anchor;
    }

    fn step_history(&mut self, ui: &mut Ui, backwards: bool) {
        let here = self.key(self.open);
        let (from, to) =
            if backwards { (&mut self.back, &mut self.forward) } else { (&mut self.forward, &mut self.back) };
        let Some(key) = from.pop() else { return };
        to.push(here);
        let found = self.library.borrow().find(&key);
        if let Some(page) = found {
            self.go(ui, page, None, false);
        }
    }

    /// Where a link in the open page goes.
    fn follow(&mut self, ui: &mut Ui, url: &str) {
        let target = self.library.borrow().resolve(url, self.open);
        match target {
            Target::Page(page, anchor) => self.go(ui, page, anchor, true),
            Target::Heading(anchor) => self.pending_anchor = Some(anchor),
            // A type no longer in the code: there is nothing to write.
            Target::Missing(name) if name.starts_with(CODE_SCHEME) => {}
            Target::Missing(name) => self.write_missing(ui, &name),
            Target::Web(url) => open_in_browser(&url),
            Target::File(path) => self.file_request = Some(path),
        }
    }

    /// A link to a page not written yet makes it, beside the page it is in,
    /// and opens it to write.
    fn write_missing(&mut self, ui: &mut Ui, name: &str) {
        let folder = self.open;
        let created = self.library.borrow_mut().create(folder, name);
        match created {
            Ok(page) => {
                ui.refresh_list(self.nodes.tree);
                self.go(ui, page, None, true);
                self.start_editing(ui);
                ui.set_text(self.nodes.source, &format!("# {name}\n\n"));
                self.dirty = true;
                self.show_status(ui);
                self.render_preview(ui);
            }
            Err(error) => tracing::error!("VoxDocs: could not make {name}: {error:#}"),
        }
    }

    /// A checklist box ticked or unticked saves the page.
    fn tick_tasks(&mut self, ui: &mut Ui) {
        let ticked = self.rendered.tasks.iter().find(|(node, _)| ui.changed(*node)).map(|(node, task)| (*node, *task));
        let Some((node, task)) = ticked else { return };
        let done = ui.is_checked(node);
        if let Err(error) = self.library.borrow_mut().set_task(self.open, task, done) {
            tracing::error!("VoxDocs: {error:#}");
        }
        self.render(ui);
    }

    /// The pages with what is typed in the find box and every tag picked,
    /// in place of the tree, or the tree when there is neither.
    fn search(&mut self, ui: &mut Ui) {
        let query = ui.text(self.nodes.search);
        let found = self.library.borrow().search_tagged(&query, &self.picked_tags);
        let searching = !query.trim().is_empty() || !self.picked_tags.is_empty();
        let count = found.len();
        *self.results.borrow_mut() = found;
        ui.set_visible(self.nodes.tree, !searching);
        ui.set_visible(self.nodes.results, searching);
        ui.set_list_count(self.nodes.results, count);
        ui.set_selection(self.nodes.results, &[]);
        ui.refresh_list(self.nodes.results);
    }

    fn select_in_tree(&mut self, ui: &mut Ui) {
        let library = self.library.borrow();
        let mut parent = library.page(self.open).parent;
        while let Some(folder) = parent {
            ui.set_expanded(self.nodes.tree, folder.0 as u64, true);
            parent = library.page(folder).parent;
        }
        drop(library);
        ui.select_item(self.nodes.tree, self.open.0 as u64);
    }

    /// Tags clicked over the tree or on the page pick or let go of them, and
    /// Show all and Clear do what they say.
    fn update_tags(&mut self, ui: &mut Ui) {
        let chip = self.tag_chips.iter().find(|(node, _)| ui.clicked(*node)).map(|(_, tag)| tag.clone());
        let on_page = self.page_tags.iter().find(|(node, _)| ui.clicked(*node)).map(|(_, tag)| tag.clone());
        let changed = if let Some(tag) = chip {
            match self.picked_tags.iter().position(|picked| picked.eq_ignore_ascii_case(&tag)) {
                Some(at) => drop(self.picked_tags.remove(at)),
                None => self.picked_tags.push(tag),
            }
            true
        } else if let Some(tag) = on_page.filter(|tag| !self.picked_tags.iter().any(|p| p.eq_ignore_ascii_case(tag))) {
            self.picked_tags.push(tag);
            true
        } else if ui.clicked(self.nodes.tags_clear) {
            self.picked_tags.clear();
            true
        } else {
            false
        };
        if ui.clicked(self.nodes.tags_more) {
            self.all_tags = !self.all_tags;
            self.show_tags(ui);
        }
        if changed {
            self.show_tags(ui);
            self.search(ui);
        }
    }

    /// The tags over the tree: the most used and any picked, or every one
    /// once asked, each with how many pages have it.
    fn show_tags(&mut self, ui: &mut Ui) {
        let tags = self.library.borrow().tags();
        self.picked_tags.retain(|picked| tags.iter().any(|(tag, _)| tag.eq_ignore_ascii_case(picked)));
        let picked = |tag: &str| self.picked_tags.iter().any(|p| p.eq_ignore_ascii_case(tag));
        let shown: Vec<&(String, usize)> = tags
            .iter()
            .enumerate()
            .filter(|(index, (tag, _))| self.all_tags || *index < TAGS_AT_FIRST || picked(tag))
            .map(|(_, tag)| tag)
            .collect();
        for child in ui.children(self.nodes.tag_chips) {
            ui.remove(child);
        }
        self.tag_chips.clear();
        for (tag, count) in &shown {
            let chip = ui.button(self.nodes.tag_chips, "");
            ui.add_class(chip, "cb-tag-chip");
            ui.set_class(chip, "picked", picked(tag));
            let name = ui.add_label(chip, tag);
            ui.add_class(name, "cb-tag-name");
            let number = ui.add_label(chip, &count.to_string());
            ui.add_class(number, "cb-tag-count");
            self.tag_chips.push((chip, tag.clone()));
        }
        ui.set_visible(self.nodes.tags, !tags.is_empty());
        ui.set_visible(self.nodes.tags_clear, !self.picked_tags.is_empty());
        ui.set_visible(self.nodes.tags_more, self.all_tags || shown.len() < tags.len());
        let more = if self.all_tags { "Show fewer".to_string() } else { format!("Show all {}", tags.len()) };
        ui.set_text(self.nodes.tags_more, &more);
    }

    /// "On this page" shows when the page has enough headings and there is
    /// room beside it; a click goes to a heading, and the one being read is
    /// picked out.
    fn update_toc(&mut self, ui: &mut Ui) {
        let roomy = ui.rect(self.nodes.reading)[2] >= TOC_ROOM;
        ui.set_visible(self.nodes.toc, roomy && !self.toc.is_empty());
        if let Some(anchor) = self.toc.iter().find(|(link, ..)| ui.clicked(*link)).map(|(.., anchor)| anchor.clone()) {
            self.pending_anchor = Some(anchor);
        }
        let reading = ui.rect(self.nodes.page)[1] + READING_LINE;
        // Above the first heading, it is the one being read towards; at the
        // very end, the last, which may never reach the top.
        let at_end = ui.scroll_max(self.nodes.page)[1] > 0.0
            && ui.scroll_offset(self.nodes.page)[1] >= ui.scroll_max(self.nodes.page)[1] - 1.0;
        let passed = self.toc.iter().rposition(|&(_, heading, _)| ui.rect(heading)[1] <= reading);
        let current = if at_end { self.toc.len().checked_sub(1) } else { passed.or(Some(0)) };
        for (index, &(link, ..)) in self.toc.iter().enumerate() {
            ui.set_class(link, "current", current == Some(index));
        }
    }

    /// Lists the page's headings, down to the third level, for "On this page".
    fn show_toc(&mut self, ui: &mut Ui) {
        for child in ui.children(self.nodes.toc) {
            ui.remove(child);
        }
        self.toc.clear();
        let headings: Vec<_> = self.rendered.headings.iter().filter(|h| h.level <= 3).cloned().collect();
        if headings.len() < TOC_FEWEST {
            return;
        }
        let title = ui.add_label(self.nodes.toc, "On this page");
        ui.add_class(title, "cb-toc-title");
        for heading in headings {
            let link = ui.add_label(self.nodes.toc, &heading.text);
            ui.add_class(link, "cb-toc-link");
            ui.add_class(link, &format!("cb-toc-{}", heading.level));
            self.toc.push((link, heading.node, heading.anchor));
        }
    }

    fn show_editing(&mut self, ui: &mut Ui, editing: bool) {
        self.editing = editing;
        self.focus_source &= editing;
        if !editing {
            self.preview_due = None;
        }
        ui.set_visible(self.nodes.reading, !editing);
        ui.set_visible(self.nodes.editor, editing);
        ui.set_text(self.nodes.edit_label, if editing { "Done" } else { "Edit" });
        ui.set_icon(self.nodes.edit_icon, if editing { "check" } else { "edit" });
    }

    fn show_status(&mut self, ui: &mut Ui) {
        ui.set_text(self.nodes.status, if self.dirty { "Unsaved changes" } else { "Saved" });
        ui.set_class(self.nodes.status, "dirty", self.dirty);
    }

    /// The Markdown being written, as it will read.
    fn render_preview(&mut self, ui: &mut Ui) {
        for child in ui.children(self.nodes.preview) {
            ui.remove(child);
        }
        let library = self.library.borrow();
        let blocks = markdown::parse(&ui.text(self.nodes.source));
        let open = self.open;
        let works = |url: &str| library.works(url, open);
        let pictures = |url: &str| self.picture(&library, url);
        view::render(ui, self.nodes.preview, &blocks, &Lookups { works: &works, pictures: &pictures });
    }

    /// The folders the open page is in, and whether Back and Forward go anywhere.
    fn show_path(&mut self, ui: &mut Ui) {
        let library = self.library.borrow();
        let mut crumbs = Vec::new();
        let mut at = library.page(self.open).parent;
        while let Some(folder) = at {
            crumbs.push(library.page(folder).title().to_string());
            at = library.page(folder).parent;
        }
        crumbs.reverse();
        ui.set_text(self.nodes.path, &crumbs.join("  /  "));
        ui.set_enabled(self.nodes.back, !self.back.is_empty());
        ui.set_enabled(self.nodes.forward, !self.forward.is_empty());
    }

    /// Draws the open page: its title, tags and date, its words, and for a
    /// folder, what is in it.
    fn render(&mut self, ui: &mut Ui) {
        for child in ui.children(self.nodes.page) {
            ui.remove(child);
        }
        self.showing_health = false;
        ui.set_checked(self.nodes.health, false);
        self.page_tags.clear();
        self.show_path(ui);
        let library = self.library.borrow();
        let page = library.page(self.open);
        ui.set_visible(self.nodes.edit, !page.generated);

        let title = ui.add_label(self.nodes.page, page.title());
        ui.add_class(title, "cb-title");
        if !page.header.tags.is_empty() || !page.header.updated.is_empty() {
            let meta = ui.panel(self.nodes.page, voxear_ui::Style::row());
            ui.add_class(meta, "cb-meta");
            for tag in &page.header.tags {
                let chip = ui.add_label(meta, tag);
                ui.add_class(chip, "cb-tag");
                self.page_tags.push((chip, tag.clone()));
            }
            if !page.header.updated.is_empty() {
                let updated = ui.add_label(meta, &format!("Updated {}", page.header.updated));
                ui.add_class(updated, "cb-updated");
            }
        }
        let empty = library.is_empty() && self.open == library.root() && page.body.trim().is_empty();
        let body = if empty { EMPTY } else { &page.body };
        let blocks = markdown::parse(body);
        let open = self.open;
        let works = |url: &str| library.works(url, open);
        let pictures = |url: &str| self.picture(&library, url);
        let rendered = view::render(ui, self.nodes.page, &blocks, &Lookups { works: &works, pictures: &pictures });
        self.rendered = rendered;

        if page.is_folder && !page.children.is_empty() {
            let heading = ui.add_label(self.nodes.page, "In this folder");
            ui.add_class(heading, "cb-section");
            for &child in &page.children {
                let child = library.page(child);
                let row = ui.panel(self.nodes.page, voxear_ui::Style::row());
                ui.add_class(row, "cb-child");
                let icon = ui.icon(row, page_icon(child));
                ui.add_class(icon, "cb-row-icon");
                let mut link = voxear_ui::RichText::new();
                link.push_link(child.title(), &format!("{PAGE_SCHEME}{}", child.reference()), Default::default());
                ui.rich_label(row, &link);
            }
        }
        self.home_health = None;
        if self.open == library.root() && !library.is_empty() {
            self.home_health = Some(show_home(ui, self.nodes.page, &library));
        }
        drop(library);
        self.show_toc(ui);
    }
    /// Docs Health in the page's place: how many things need looking after,
    /// then each kind, every entry a link to its page or file.
    fn show_health(&mut self, ui: &mut Ui) {
        for child in ui.children(self.nodes.page).into_iter().chain(ui.children(self.nodes.toc)) {
            ui.remove(child);
        }
        (self.toc, self.page_tags, self.rendered) = (Vec::new(), Vec::new(), Rendered::default());
        self.showing_health = true;
        ui.set_checked(self.nodes.health, true);
        ui.set_visible(self.nodes.edit, false);
        ui.set_scroll(self.nodes.page, [0.0, 0.0]);
        let library = self.library.borrow();
        let health = Health::check(&library);
        let page = self.nodes.page;
        let title = ui.add_label(page, "Docs Health");
        ui.add_class(title, "cb-title");
        let summary = match health.problems() {
            0 => "Nothing needs looking after.".to_string(),
            1 => "One thing needs looking after.".to_string(),
            count => format!("{count} things need looking after."),
        };
        let summary = ui.add_label(page, &summary);
        ui.add_class(summary, "md-p");
        let link_to = |id: PageId| {
            (library.page(id).title().to_string(), format!("{PAGE_SCHEME}{}", library.page(id).reference()))
        };
        let section = |ui: &mut Ui, heading: &str, about: &str, lines: Vec<voxear_ui::RichText>| {
            if lines.is_empty() {
                return;
            }
            let head = ui.add_label(page, heading);
            ui.add_class(head, "md-h");
            ui.add_class(head, "md-h2");
            let note = ui.add_label(page, about);
            ui.add_class(note, "cb-health-about");
            for line in lines {
                let row = ui.rich_label(page, &line);
                ui.add_class(row, "md-p");
            }
        };
        let page_link = |text: &mut voxear_ui::RichText, id: PageId| {
            let (title, url) = link_to(id);
            text.push_link(&title, &url, Default::default());
        };
        let code = voxear_ui::Marks { code: true, ..Default::default() };
        let stale = health.stale.iter().map(|(id, files)| {
            let mut text = voxear_ui::RichText::new();
            page_link(&mut text, *id);
            text.push(" · ", Default::default());
            for (index, file) in files.iter().enumerate() {
                if index > 0 {
                    text.push(", ", Default::default());
                }
                text.push_link(file, file, Default::default());
            }
            text
        });
        let stale = stale.collect();
        section(ui, "Out of date", "Pages that link to files changed since the page was last updated.", stale);
        let broken = health.broken.iter().map(|(id, url)| {
            let mut text = voxear_ui::RichText::new();
            page_link(&mut text, *id);
            text.push(" links to ", Default::default());
            text.push(url, code);
            text
        });
        let broken = broken.collect();
        section(ui, "Links that go nowhere", "A page not written yet is written by clicking its link.", broken);
        let missing = health.missing_pictures.iter().map(|(id, url)| {
            let mut text = voxear_ui::RichText::new();
            page_link(&mut text, *id);
            text.push(" shows ", Default::default());
            text.push(url, code);
            text
        });
        let missing = missing.collect();
        section(ui, "Missing pictures", "Pictures a page shows that are not there.", missing);
        let unused = health.unused_pictures.iter().map(|path| {
            let mut text = voxear_ui::RichText::new();
            text.push_link(path, path, Default::default());
            text
        });
        let unused = unused.collect();
        section(
            ui,
            "Pictures no page shows",
            "Kept in the pages' images folders, with nothing pointing at them.",
            unused,
        );
    }

    /// Scrolls to the heading a link named, once the page is laid out.
    fn scroll_to_pending(&mut self, ui: &mut Ui) {
        let Some(anchor) = self.pending_anchor.as_deref() else { return };
        let Some(heading) = self.rendered.headings.iter().find(|h| h.anchor == anchor).map(|h| h.node) else {
            self.pending_anchor = None;
            return;
        };
        let [_, top, _, height] = ui.rect(heading);
        if height <= 0.0 {
            return;
        }
        let view = ui.rect(self.nodes.page)[1];
        let scrolled = ui.scroll_offset(self.nodes.page)[1];
        ui.set_scroll(self.nodes.page, [0.0, (top - view + scrolled - 8.0).max(0.0)]);
        self.pending_anchor = None;
    }
}

/// Opens a web address in the system's browser.
fn open_in_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(error) = opener {
        tracing::warn!("VoxDocs: could not open {url}: {error}");
    }
}

/// Home's lists, under the top page in `parent`: the pages updated most
/// lately, with when, and a line saying how healthy the docs are, which
/// opens Health and is returned.
fn show_home(ui: &mut Ui, parent: NodeId, library: &Library) -> NodeId {
    let mut recent: Vec<(&str, PageId)> = library
        .pages()
        .filter(|(id, page)| *id != library.root() && !page.header.updated.is_empty())
        .map(|(id, page)| (page.header.updated.as_str(), id))
        .collect();
    recent.sort_by(|a, b| b.0.cmp(a.0).then_with(|| library.page(a.1).title().cmp(library.page(b.1).title())));
    if !recent.is_empty() {
        let heading = ui.add_label(parent, "Recently updated");
        ui.add_class(heading, "cb-section");
        for (updated, id) in recent.into_iter().take(RECENT) {
            let page = library.page(id);
            let row = ui.panel(parent, voxear_ui::Style::row());
            ui.add_class(row, "cb-child");
            let icon = ui.icon(row, page_icon(page));
            ui.add_class(icon, "cb-row-icon");
            let mut link = voxear_ui::RichText::new();
            link.push_link(page.title(), &format!("{PAGE_SCHEME}{}", page.reference()), Default::default());
            let title = ui.rich_label(row, &link);
            ui.add_class(title, "cb-home-title");
            let date = ui.add_label(row, updated);
            ui.add_class(date, "cb-updated");
        }
    }
    let heading = ui.add_label(parent, "Docs health");
    ui.add_class(heading, "cb-section");
    let line = match Health::check(library).problems() {
        0 => "Nothing needs looking after".to_string(),
        1 => "One thing needs looking after: show it".to_string(),
        count => format!("{count} things need looking after: show them"),
    };
    let line = ui.add_label(parent, &line);
    ui.add_class(line, "cb-home-health");
    line
}
