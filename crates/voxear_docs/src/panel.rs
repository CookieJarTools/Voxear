//! The VoxDocs panel: the pages down the side and the page beside them,
//! as in the CookieJarTools Cookbook. Click a page to read it; links go to
//! other pages, headings, files and the web, and a link to a page not
//! written yet makes it. Ticking a checklist box saves the page. Back and
//! forward go through the pages read, and the box at the top finds pages by
//! their words. Edit shows the page's Markdown to change and save.

use std::{cell::RefCell, rc::Rc};

use voxear_ui::{ItemFns, NodeId, TreeItems, Ui};

use crate::{
    library::{Library, PageId, Target},
    markdown::{self, PAGE_SCHEME},
    view::{self, Rendered},
};

const MARKUP: &str = include_str!("../ui/VoxDocs.vui");
const SHEET: &str = include_str!("../ui/VoxDocs.vss");

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
        ui.set_icon(parts[0], if page.is_folder { "folder" } else { "file" });
        ui.set_text(parts[1], page.title());
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
    edit: NodeId,
    save: NodeId,
    cancel: NodeId,
    page: NodeId,
    source: NodeId,
}

pub struct VoxDocs {
    library: Shared,
    nodes: Nodes,
    open: PageId,
    back: Vec<PageId>,
    forward: Vec<PageId>,
    rendered: Rendered,
    editing: bool,
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
            edit: find(ui, "cb-edit"),
            save: find(ui, "cb-save"),
            cancel: find(ui, "cb-cancel"),
            page: find(ui, "cb-page"),
            source: find(ui, "cb-source"),
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
                ui.set_icon(parts[0], if page.is_folder { "folder" } else { "file" });
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
            pending_anchor: None,
            results,
            file_request: None,
        };
        docs.show_editing(ui, false);
        docs.render(ui);
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

    /// A file in the project that a link asked to show, once.
    pub fn take_file_request(&mut self) -> Option<String> {
        self.file_request.take()
    }

    /// Answers what was clicked and typed since the last frame.
    pub fn update(&mut self, ui: &mut Ui) {
        self.scroll_to_pending(ui);
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
        if ui.clicked(self.nodes.back) {
            self.step_history(ui, true);
        }
        if ui.clicked(self.nodes.forward) {
            self.step_history(ui, false);
        }
        if let Some(url) = ui.clicked_link(self.nodes.page).map(str::to_string) {
            self.follow(ui, &url);
        }
        self.tick_tasks(ui);
        let copied = self.rendered.copies.iter().find(|(button, _)| ui.clicked(*button)).map(|(_, code)| code.clone());
        if let Some(code) = copied {
            ui.copy_text(&code);
        }
        if ui.clicked(self.nodes.edit) {
            let body = self.library.borrow().page(self.open).body.clone();
            ui.set_text(self.nodes.source, &body);
            self.show_editing(ui, true);
            ui.focus(self.nodes.source);
        }
        if ui.clicked(self.nodes.cancel) {
            self.show_editing(ui, false);
        }
        if ui.clicked(self.nodes.save) {
            let body = ui.text(self.nodes.source);
            if let Err(error) = self.library.borrow_mut().save(self.open, &body) {
                tracing::error!("VoxDocs: could not save the page: {error:#}");
            }
            self.show_editing(ui, false);
            self.render(ui);
        }
    }

    /// Opens a page, remembering the one left for Back when `record` is set.
    fn go(&mut self, ui: &mut Ui, page: PageId, anchor: Option<String>, record: bool) {
        if page != self.open {
            if record {
                self.back.push(self.open);
                self.forward.clear();
            }
            self.open = page;
            self.show_editing(ui, false);
            self.render(ui);
            ui.set_scroll(self.nodes.page, [0.0, 0.0]);
            self.select_in_tree(ui);
        }
        self.pending_anchor = anchor;
    }

    fn step_history(&mut self, ui: &mut Ui, backwards: bool) {
        let (from, to) =
            if backwards { (&mut self.back, &mut self.forward) } else { (&mut self.forward, &mut self.back) };
        let Some(page) = from.pop() else { return };
        to.push(self.open);
        self.go(ui, page, None, false);
    }

    /// Where a link in the open page goes.
    fn follow(&mut self, ui: &mut Ui, url: &str) {
        let target = self.library.borrow().resolve(url, self.open);
        match target {
            Target::Page(page, anchor) => self.go(ui, page, anchor, true),
            Target::Heading(anchor) => self.pending_anchor = Some(anchor),
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
                ui.set_text(self.nodes.source, &format!("# {name}\n\n"));
                self.show_editing(ui, true);
                ui.focus(self.nodes.source);
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

    fn search(&mut self, ui: &mut Ui) {
        let query = ui.text(self.nodes.search);
        let found = self.library.borrow().search(&query);
        let searching = !query.trim().is_empty();
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

    fn show_editing(&mut self, ui: &mut Ui, editing: bool) {
        self.editing = editing;
        ui.set_visible(self.nodes.page, !editing);
        ui.set_visible(self.nodes.source, editing);
        ui.set_visible(self.nodes.edit, !editing);
        ui.set_visible(self.nodes.save, editing);
        ui.set_visible(self.nodes.cancel, editing);
    }

    /// Draws the open page: its title, tags and date, its words, and for a
    /// folder, what is in it.
    fn render(&mut self, ui: &mut Ui) {
        for child in ui.children(self.nodes.page) {
            ui.remove(child);
        }
        let library = self.library.borrow();
        let page = library.page(self.open);
        let mut crumbs = Vec::new();
        let mut at = page.parent;
        while let Some(folder) = at {
            crumbs.push(library.page(folder).title().to_string());
            at = library.page(folder).parent;
        }
        crumbs.reverse();
        ui.set_text(self.nodes.path, &crumbs.join("  /  "));
        ui.set_enabled(self.nodes.back, !self.back.is_empty());
        ui.set_enabled(self.nodes.forward, !self.forward.is_empty());

        let title = ui.add_label(self.nodes.page, page.title());
        ui.add_class(title, "cb-title");
        if !page.header.tags.is_empty() || !page.header.updated.is_empty() {
            let meta = ui.panel(self.nodes.page, voxear_ui::Style::row());
            ui.add_class(meta, "cb-meta");
            for tag in &page.header.tags {
                let chip = ui.add_label(meta, tag);
                ui.add_class(chip, "cb-tag");
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
        self.rendered = view::render(ui, self.nodes.page, &blocks, &works);

        if page.is_folder && !page.children.is_empty() {
            let heading = ui.add_label(self.nodes.page, "In this folder");
            ui.add_class(heading, "cb-section");
            for &child in &page.children {
                let child = library.page(child);
                let row = ui.panel(self.nodes.page, voxear_ui::Style::row());
                ui.add_class(row, "cb-child");
                let icon = ui.icon(row, if child.is_folder { "folder" } else { "file" });
                ui.add_class(icon, "cb-row-icon");
                let mut link = voxear_ui::RichText::new();
                link.push_link(child.title(), &format!("{PAGE_SCHEME}{}", child.reference()), Default::default());
                ui.rich_label(row, &link);
            }
        }
    }

    /// Scrolls to the heading a link named, once the page is laid out.
    fn scroll_to_pending(&mut self, ui: &mut Ui) {
        let Some(anchor) = self.pending_anchor.as_deref() else { return };
        let Some(&(heading, _)) = self.rendered.headings.iter().find(|(_, a)| a == anchor) else {
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
