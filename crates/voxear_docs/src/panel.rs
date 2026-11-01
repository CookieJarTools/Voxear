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
//! **Edit** writes the page: its title, its tags as chips (the project's
//! tags that fit what is typed are suggested), its Markdown, and the page
//! as it will read beside it, drawn again a moment after each change. Save
//! or Ctrl+S writes it to its file, and so does Done, or opening another
//! page, so nothing written is lost. A new title renames the page's file.
//!
//! The top page is home, as the Cookbook's is: under its own words and what
//! is in it, the pages updated most lately, and how healthy the docs are.
//!
//! A page's menu in the tree copies what points at it: a link to paste in
//! another page, and for a page of the project's own, a `[Documentation]`
//! attribute to paste on code, naming it by its id so it keeps working when
//! the page is renamed or moved.
//!
//! **Health** shows what needs looking after (see [`crate::health`]) in the
//! page's place, each entry a link to the page or file it is about.
//!
//! The **Code Reference** comes last in the tree once the editor hands it
//! over with [`set_code`]. Its pages are made from the code; Edit on a
//! type's page writes its note, which the page shows under Notes.
//! A page lists the code whose `[Documentation]` or `[Excerpt]` points at it
//! under **In the code**, and [`render_excerpt`] draws an excerpt for the
//! Inspector.
//!
//! **Guides**: a link to an action or a check alone in its paragraph is a
//! step. An action's button asks the editor to run the command it names
//! ([`take_action_request`]), and is ticked once it has; a check ticks
//! while what it looks for is there ([`wanted_checks`], [`set_check`]).
//! A page with steps says how many are done, and Reset starts it again.
//! What each person has done is kept on their machine (see
//! [`crate::progress`]).
//!
//! **Live assets** show their previews, as pictures; a material's fields
//! show under it too, in a place the editor fills ([`live_fields`]).
//!
//! **Pictures** are files beside a page, or anywhere in the project. The
//! panel cannot read them into textures itself: [`wanted_pictures`] lists
//! the files a page needs, and [`set_picture`] hands each back once the
//! editor has loaded it, which draws the page again.
//!
//! [`wanted_pictures`]: VoxDocs::wanted_pictures
//! [`set_picture`]: VoxDocs::set_picture
//! [`set_code`]: VoxDocs::set_code
//! [`live_fields`]: VoxDocs::live_fields
//! [`take_action_request`]: VoxDocs::take_action_request
//! [`wanted_checks`]: VoxDocs::wanted_checks
//! [`set_check`]: VoxDocs::set_check
//! [`render_excerpt`]: VoxDocs::render_excerpt

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

use voxear_ui::{ImageRef, ItemFns, MenuItem, NodeId, TreeItems, Ui};

use crate::{
    code::{CODE_SCHEME, CodeReference},
    health::Health,
    library::{Excerpt, Library, Page, PageId, Target, unescape},
    markdown::{self, PAGE_SCHEME},
    progress::Progress,
    view::{self, ACTION_SCHEME, CHECK_SCHEME, Lookups, Picture, Rendered},
};

const MARKUP: &str = include_str!("../ui/VoxDocs.vui");
const SHEET: &str = include_str!("../ui/VoxDocs.vss");

/// How many pages home lists as updated lately.
const RECENT: usize = 6;
/// How many tags show before Show all, beside any picked; past it, a box
/// finds a tag among them.
const TAGS_AT_FIRST: usize = 8;
/// How many of the project's tags are suggested while writing.
const SUGGESTED: usize = 6;
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
    tag_find: NodeId,
    tag_picked: NodeId,
    tag_suggestions: NodeId,
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
    insert_image: NodeId,
    insert_link: NodeId,
    insert_action: NodeId,
    capture_scene: NodeId,
    record_scene: NodeId,
    record_game: NodeId,
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
    /// How far each guide has got, what the checks last found, by what
    /// they look for, the action asked for, with its page and step, and
    /// the one being run.
    progress: Progress,
    checks: HashMap<String, Result<(), String>>,
    action_request: Option<(String, String, String)>,
    running: Option<(String, String)>,
    /// A guide's line saying how many steps are done, and its Reset.
    progress_line: Option<(NodeId, NodeId)>,
    /// What each item of a page's menu in the tree copies.
    page_menu: Vec<String>,
    /// Where the page and the preview show each picture, by its file, to
    /// swap a new frame in.
    page_pictures: Vec<(PathBuf, NodeId)>,
    preview_pictures: Vec<(PathBuf, NodeId)>,
    /// While writing: the open menu of Image…, Link… or Action…, by its
    /// button, with the Markdown each item puts in; and Action… asked for
    /// its menu, which the editor fills with its commands.
    insert_menu: Option<(NodeId, Vec<String>)>,
    wants_commands: bool,
    /// While writing: the page's tags, each chip's × with its tag, and the
    /// suggested tags with theirs.
    writing_tags: Vec<String>,
    tag_removes: Vec<(NodeId, String)>,
    suggestions: Vec<(NodeId, String)>,
    /// A picture of the Scene or a recording asked for, for the editor; and
    /// Markdown to put in at the cursor once the page shows again, as after
    /// one, when the view took the page's place.
    capture: Option<Capture>,
    pending_insert: Option<String>,
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
            tag_find: find(ui, "cb-tag-find"),
            tag_picked: find(ui, "cb-tag-picked"),
            tag_suggestions: find(ui, "cb-tag-suggestions"),
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
            insert_image: find(ui, "cb-insert-image"),
            insert_link: find(ui, "cb-insert-link"),
            insert_action: find(ui, "cb-insert-action"),
            capture_scene: find(ui, "cb-capture-scene"),
            record_scene: find(ui, "cb-record-scene"),
            record_game: find(ui, "cb-record-game"),
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
            progress: Progress::open(project),
            checks: HashMap::new(),
            action_request: None,
            running: None,
            progress_line: None,
            page_menu: Vec::new(),
            page_pictures: Vec::new(),
            preview_pictures: Vec::new(),
            insert_menu: None,
            wants_commands: false,
            capture: None,
            pending_insert: None,
            writing_tags: Vec::new(),
            tag_removes: Vec::new(),
            suggestions: Vec::new(),
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
    ///
    /// A picture already shown that comes again at the same size, as a GIF's
    /// next frame or a live asset's preview moved, is swapped in where it
    /// shows, rather than the page being drawn again.
    pub fn set_picture(&mut self, ui: &mut Ui, path: &Path, picture: Option<(ImageRef, [u32; 2])>) {
        let shown = picture.map_or(Picture::Missing, |(image, size)| Picture::Shown(image, size));
        let was = self.pictures.insert(path.to_path_buf(), shown);
        if let (Some(Picture::Shown(_, before)), Picture::Shown(image, now)) = (was, shown)
            && before == now
        {
            let drawn = self.page_pictures.iter().chain(&self.preview_pictures);
            for (_, node) in drawn.filter(|(at, _)| at == path) {
                ui.set_image(*node, Some(image));
            }
            return;
        }
        self.redraw = true;
    }

    /// How a picture in the open page shows: once read, or else as loading,
    /// noting it to be read, or as missing.
    fn picture(&self, library: &Library, page: PageId, url: &str) -> Picture {
        let Some(path) = library.picture_path(url, page) else { return Picture::Missing };
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
        // The chips, and a tag typed but not yet made one.
        let typed = ui.text(self.nodes.tags_field);
        let tags: Vec<String> = self.writing_tags.iter().cloned().chain(typed.split(',').map(str::to_string)).collect();
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

    /// A file dropped on the editor while a page is being written, put in
    /// the page at the cursor. A picture from outside the project is copied
    /// into an `images` folder beside the page; one in the project, or a
    /// prefab, model or material, is shown from where it is; any other file
    /// of the project is linked. Returns whether the page took it.
    pub fn drop_file(&mut self, ui: &mut Ui, path: &Path) -> bool {
        if !self.editing {
            return false;
        }
        let library = self.library.borrow();
        let in_project = path.strip_prefix(library.project()).ok().map(|p| p.to_string_lossy().replace('\\', "/"));
        let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let shown = PICTURES.contains(&extension.as_str()) || LIVE.contains(&extension.as_str());
        let markdown = match in_project {
            Some(within) if shown => alone(&format!("![{name}]({})", address(&within))),
            Some(within) => format!("[{name}]({})", address(&within)),
            None if PICTURES.contains(&extension.as_str()) => {
                let folder = library.folder().join(library.page(self.open).directory()).join("images");
                match copy_into(path, &folder) {
                    Ok(file) => alone(&format!("![{name}]({})", address(&format!("images/{file}")))),
                    Err(error) => {
                        tracing::error!("VoxDocs: could not keep {} with the page: {error:#}", path.display());
                        return false;
                    }
                }
            }
            None => return false,
        };
        drop(library);
        if ui.focused() != Some(self.nodes.source) {
            ui.focus(self.nodes.source);
            ui.caret_to_end(self.nodes.source);
        }
        ui.text_input(&markdown);
        true
    }

    /// Whether Action… was clicked, once: the editor then hands over its
    /// commands with [`open_action_menu`](Self::open_action_menu).
    pub fn take_commands_wanted(&mut self) -> bool {
        std::mem::take(&mut self.wants_commands)
    }

    /// Action…'s menu: the three kinds of check, then a step for each of
    /// `commands`, by their titles as the command bar lists them.
    pub fn open_action_menu(&mut self, ui: &mut Ui, commands: &[String]) {
        let examples = [
            ("Check: there is an Obj called…", "[There is a Sun](check:obj/Sun)"),
            ("Check: an Obj has a component…", "[The level is lit](check:component/Light)"),
            ("Check: the project has a file…", "[The lamp is there](check:file/Prefabs/Lamp.prefab)"),
            ("Make an asset…", "[Make the brass](<action:create/Material?at=Materials/Brass.material>)"),
        ];
        let mut items: Vec<(String, String)> =
            examples.iter().map(|(label, step)| (label.to_string(), alone(step))).collect();
        for title in commands {
            let step = format!("[{title}]({})", address(&format!("{ACTION_SCHEME}{title}")));
            items.push((title.clone(), alone(&step)));
        }
        self.open_insert_menu(ui, self.nodes.insert_action, items);
    }

    /// Image…, Link… and Action… while writing, and what their menus put in.
    fn update_inserts(&mut self, ui: &mut Ui) {
        if let Some((owner, texts)) = &self.insert_menu
            && let Some(text) = ui.menu_choice(*owner).and_then(|at| texts.get(at)).cloned()
        {
            self.insert_menu = None;
            if ui.focused() != Some(self.nodes.source) {
                ui.focus(self.nodes.source);
            }
            ui.text_input(&text);
        }
        if ui.clicked(self.nodes.insert_image) {
            let items = self.picture_items();
            self.open_insert_menu(ui, self.nodes.insert_image, items);
        }
        if ui.clicked(self.nodes.insert_link) {
            let items = self.link_items();
            self.open_insert_menu(ui, self.nodes.insert_link, items);
        }
        if ui.clicked(self.nodes.insert_action) {
            self.wants_commands = true;
        }
        let captures = [
            (self.nodes.capture_scene, Capture::Scene),
            (self.nodes.record_scene, Capture::SceneRecording),
            (self.nodes.record_game, Capture::GameRecording),
        ];
        if let Some(&(_, capture)) = captures.iter().find(|(button, _)| ui.clicked(*button)) {
            self.capture = Some(capture);
        }
    }

    /// What Capture Scene, Record Scene or Record Game asked for, once.
    pub fn take_capture_request(&mut self) -> Option<Capture> {
        self.capture.take()
    }

    /// Keeps a picture made for the page being written, a capture or a
    /// recording, in the `images` folder beside it as `name` and `extension`
    /// (a number added if a different file has the name), and puts it in at
    /// the cursor once the page shows. Returns its path from the project.
    pub fn keep_picture(&mut self, name: &str, extension: &str, bytes: &[u8]) -> std::io::Result<String> {
        let library = self.library.borrow();
        let directory = library.page(self.open).directory().to_string();
        let folder = library.folder().join(&directory).join("images");
        let kept = keep_into(&folder, name, extension, bytes)?;
        let in_project = library.folder().join(&directory).join("images").join(&kept);
        let in_project =
            in_project.strip_prefix(library.project()).unwrap_or(&in_project).to_string_lossy().replace('\\', "/");
        drop(library);
        if self.editing {
            let markdown = alone(&format!("![{name}]({})", address(&format!("images/{kept}"))));
            self.pending_insert.get_or_insert_default().push_str(&markdown);
        }
        Ok(in_project)
    }

    /// A menu under a button of the writing tools, of labels and the
    /// Markdown each puts in.
    fn open_insert_menu(&mut self, ui: &mut Ui, button: NodeId, items: Vec<(String, String)>) {
        let [x, y, _, height] = ui.rect(button);
        let menu: Vec<MenuItem> = if items.is_empty() {
            vec![MenuItem::new("Nothing to put in").enabled(false)]
        } else {
            items.iter().map(|(label, _)| MenuItem::new(label)).collect()
        };
        ui.open_menu(button, (x, y + height + 4.0), &menu);
        self.insert_menu = Some((button, items.into_iter().map(|(_, text)| text).collect()));
    }

    /// The project's pictures and the assets that show live, by their paths.
    fn picture_items(&self) -> Vec<(String, String)> {
        let library = self.library.borrow();
        let mut files = Vec::new();
        shown_files(library.project(), library.project(), &mut files);
        files.sort();
        files
            .into_iter()
            .map(|path| {
                let name = path.rsplit('/').next().unwrap_or(&path).rsplit_once('.').map_or("", |(n, _)| n).to_string();
                (path.clone(), alone(&format!("![{name}]({})", address(&path))))
            })
            .collect()
    }

    /// The project's own pages, linked by their titles, or by their paths
    /// when another page shares the title.
    fn link_items(&self) -> Vec<(String, String)> {
        let library = self.library.borrow();
        let mut pages: Vec<(String, String)> = library
            .pages()
            .filter(|(id, page)| !page.generated && *id != library.root() && *id != self.open)
            .map(|(id, page)| {
                let title = page.title();
                let link = if library.find(title) == Some(id) {
                    format!("[[{title}]]")
                } else {
                    format!("[[{}|{title}]]", page.reference())
                };
                (page.reference().to_string(), link)
            })
            .collect();
        pages.sort();
        pages
    }

    /// The places under the open page's live assets for their fields, with
    /// each asset's file, for the editor to fill. They change whenever the
    /// page is drawn again.
    pub fn live_fields(&self) -> Vec<(PathBuf, NodeId)> {
        let library = self.library.borrow();
        let fields = self.rendered.fields.iter();
        fields.filter_map(|(url, place)| Some((library.picture_path(url, self.open)?, *place))).collect()
    }

    /// The command a step's button asked to run, by its title, once. Call
    /// [`action_ran`](Self::action_ran) when it has.
    pub fn take_action_request(&mut self) -> Option<String> {
        let (page, step, command) = self.action_request.take()?;
        self.running = Some((page, step));
        Some(command)
    }

    /// Ticks the step whose command was asked for, as having run.
    pub fn action_ran(&mut self, ui: &mut Ui) {
        if let Some((page, step)) = self.running.take() {
            self.progress.set_done(&page, &step);
            self.show_steps(ui);
        }
    }

    /// What the open page's checks look for, as `obj/Sun`, for the editor
    /// to look at and answer with [`set_check`](Self::set_check).
    pub fn wanted_checks(&self) -> Vec<String> {
        if self.editing || self.showing_health {
            return Vec::new();
        }
        let checks = self.rendered.steps.iter().filter_map(|step| step.url.strip_prefix(CHECK_SCHEME));
        checks.map(unescape).collect()
    }

    /// What a check found: nothing wrong, or what is still to do.
    pub fn set_check(&mut self, ui: &mut Ui, check: &str, found: Result<(), String>) {
        if self.checks.get(check) != Some(&found) {
            self.checks.insert(check.to_string(), found);
            self.show_steps(ui);
        }
    }

    /// Ticks the steps done, says what checks still want, and how far the
    /// guide has got.
    fn show_steps(&mut self, ui: &mut Ui) {
        let page = self.key(self.open);
        let mut done = 0;
        for step in &self.rendered.steps {
            let (passed, note) = match step.url.strip_prefix(CHECK_SCHEME) {
                Some(check) => match self.checks.get(&unescape(check)) {
                    Some(Ok(())) => (true, None),
                    Some(Err(still)) => (false, Some(still.as_str())),
                    None => (false, None),
                },
                None => (self.progress.is_done(&page, &step.url), None),
            };
            done += usize::from(passed);
            ui.set_class(step.row, "md-step-done", passed);
            ui.set_text(step.note, note.unwrap_or_default());
            ui.set_visible(step.note, note.is_some());
        }
        if let Some((line, _)) = self.progress_line {
            ui.set_text(line, &format!("{done} of {} steps done", self.rendered.steps.len()));
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
        self.update_page_menu(ui);
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
        let asked = self.rendered.steps.iter().find(|step| step.button.is_some_and(|b| ui.clicked(b)));
        if let Some(step) = asked {
            let command = unescape(step.url.strip_prefix(ACTION_SCHEME).unwrap_or_default());
            self.action_request = Some((self.key(self.open), step.url.clone(), command));
        }
        if self.progress_line.is_some_and(|(_, reset)| ui.clicked(reset)) {
            self.progress.reset(&self.key(self.open));
            self.show_steps(ui);
        }
        let copied = self.rendered.copies.iter().find(|(button, _)| ui.clicked(*button)).map(|(_, code)| code.clone());
        if let Some(code) = copied {
            ui.copy_text(&code);
        }
        if self.home_health.is_some_and(|line| ui.clicked(line)) {
            self.show_health(ui);
        }
        if ui.clicked(self.nodes.health) {
            // Asked for, Health looks at git again, for commits made since it last did.
            self.library.borrow().forget_history();
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
        if let Some(markdown) = self.pending_insert.take() {
            ui.focus(self.nodes.source);
            if ui.focused() == Some(self.nodes.source) {
                ui.text_input(&markdown);
            } else {
                self.pending_insert = Some(markdown);
            }
        }
        self.update_inserts(ui);
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
        if self.update_writing_tags(ui) && !self.dirty {
            self.dirty = true;
            self.show_status(ui);
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

    /// The page's tags while writing: what is typed becomes a chip at a comma
    /// or Enter, a chip's × takes its tag off, and a suggestion adds its
    /// own. Returns whether the tags changed.
    fn update_writing_tags(&mut self, ui: &mut Ui) -> bool {
        let field = self.nodes.tags_field;
        let mut added = Vec::new();
        if ui.submitted(field) {
            added.push(ui.text(field));
            ui.set_text(field, "");
        } else if ui.changed(field)
            && let Some((done, typing)) = ui.text(field).rsplit_once(',')
        {
            added.extend(done.split(',').map(str::to_string));
            ui.set_text(field, typing.trim_start());
        }
        let suggested = self.suggestions.iter().find(|(chip, _)| ui.clicked(*chip)).map(|(_, tag)| tag.clone());
        if let Some(tag) = suggested {
            added.push(tag);
            ui.set_text(field, "");
            ui.focus(field);
        }
        let mut changed = false;
        for tag in added.iter().map(|tag| tag.trim()).filter(|tag| !tag.is_empty()) {
            if !self.writing_tags.iter().any(|has| has.eq_ignore_ascii_case(tag)) {
                self.writing_tags.push(tag.to_string());
                changed = true;
            }
        }
        if let Some(gone) = self.tag_removes.iter().find(|(button, _)| ui.clicked(*button)).map(|(_, tag)| tag.clone())
        {
            self.writing_tags.retain(|tag| *tag != gone);
            changed = true;
        }
        if changed {
            self.show_writing_tags(ui);
        } else if ui.changed(field) {
            self.show_suggestions(ui);
        }
        changed
    }

    /// The page's tags as chips, each with its ×.
    fn show_writing_tags(&mut self, ui: &mut Ui) {
        for child in ui.children(self.nodes.tag_picked) {
            ui.remove(child);
        }
        self.tag_removes.clear();
        for tag in &self.writing_tags {
            let chip = ui.panel(self.nodes.tag_picked, voxear_ui::Style::row());
            ui.add_class(chip, "cb-tag-chip");
            let name = ui.add_label(chip, tag);
            ui.add_class(name, "cb-tag-name");
            let remove = ui.icon_button(chip, "close");
            ui.add_class(remove, "cb-tag-remove");
            self.tag_removes.push((remove, tag.clone()));
        }
        self.show_suggestions(ui);
    }

    /// The project's tags the page has not, that fit what is typed, the most
    /// used first: or with nothing typed, the most used.
    fn show_suggestions(&mut self, ui: &mut Ui) {
        for child in ui.children(self.nodes.tag_suggestions) {
            ui.remove(child);
        }
        self.suggestions.clear();
        let typed = ui.text(self.nodes.tags_field).trim().to_lowercase();
        let tags = self.library.borrow().tags();
        let has = |tag: &str| self.writing_tags.iter().any(|t| t.eq_ignore_ascii_case(tag));
        let fitting: Vec<String> = tags
            .into_iter()
            .map(|(tag, _)| tag)
            .filter(|tag| !has(tag) && tag.to_lowercase().contains(&typed))
            .take(SUGGESTED)
            .collect();
        ui.set_visible(self.nodes.tag_suggestions, !fitting.is_empty());
        if fitting.is_empty() {
            return;
        }
        let title = ui.add_label(self.nodes.tag_suggestions, "Suggested:");
        ui.add_class(title, "cb-tag-suggestions-title");
        for tag in fitting {
            let chip = ui.button(self.nodes.tag_suggestions, "");
            ui.add_class(chip, "cb-tag-chip");
            let name = ui.add_label(chip, &tag);
            ui.add_class(name, "cb-tag-name");
            self.suggestions.push((chip, tag));
        }
    }

    /// Writes the open page: its title, tags and Markdown in fields, and how
    /// it reads beside them.
    fn start_editing(&mut self, ui: &mut Ui) {
        let library = self.library.borrow();
        let page = library.page(self.open);
        let is_root = self.open == library.root();
        ui.set_text(self.nodes.title, if is_root { "" } else { page.title() });
        // A Code Reference page keeps its type's name, and what is written on it is its note.
        ui.set_enabled(self.nodes.title, !is_root && !page.generated);
        self.writing_tags.clone_from(&page.header.tags);
        ui.set_text(self.nodes.tags_field, "");
        let words = if page.generated { library.notes_of(self.open) } else { page.body.clone() };
        ui.set_text(self.nodes.source, &words);
        drop(library);
        self.show_writing_tags(ui);
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

    /// Draws what `[Excerpt]` shows in `parent`, as the Inspector does
    /// above a field: the page's words, with its links and pictures.
    pub fn render_excerpt(&self, ui: &mut Ui, parent: NodeId, excerpt: &Excerpt) {
        // The page's look, which VoxDocs' sheet gives only what is inside the panel.
        ui.clear_sheets(parent);
        ui.add_sheet(parent, voxear_ui::StyleSheet::parse("VoxDocs.vss", SHEET));
        let library = self.library.borrow();
        let works = |url: &str| library.works(url, excerpt.page);
        let pictures = |url: &str| self.picture(&library, excerpt.page, url);
        view::render(ui, parent, &excerpt.blocks, &Lookups { works: &works, pictures: &pictures });
    }

    /// Follows a link clicked outside the panel, as in an excerpt, from the
    /// page it is on: another page opens here.
    pub fn follow_from(&mut self, ui: &mut Ui, page: PageId, url: &str) {
        let target = self.library.borrow().resolve(url, page);
        if let Target::Heading(anchor) = target {
            self.go(ui, page, Some(anchor), true);
            return;
        }
        self.follow(ui, url);
    }

    /// A page's menu in the tree, from a right click on it, and what its
    /// items copy.
    fn update_page_menu(&mut self, ui: &mut Ui) {
        if let Some(text) = ui.menu_choice(self.nodes.tree).and_then(|at| self.page_menu.get(at)) {
            ui.copy_text(text);
        }
        if !ui.context_clicked(self.nodes.tree) {
            return;
        }
        let Some(item) = ui.item_at_pointer(self.nodes.tree) else { return };
        let library = self.library.borrow();
        let id = PageId(item as usize);
        if id.0 >= library.len() {
            return;
        }
        let page = library.page(id);
        let mut items = vec![(MenuItem::new("Copy link"), format!("[[{}]]", page.reference()))];
        if !page.generated && id != library.root() {
            let name = if page.header.id.is_empty() { page.reference() } else { page.header.id.as_str() };
            items.push((MenuItem::new("Copy [Documentation] attribute"), format!("[Documentation(\"{name}\")]")));
        }
        drop(library);
        let at = ui.pointer_position().unwrap_or_default();
        let menu: Vec<MenuItem> = items.iter().map(|(item, _)| item.clone()).collect();
        ui.open_menu(self.nodes.tree, at, &menu);
        self.page_menu = items.into_iter().map(|(_, text)| text).collect();
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
        if ui.changed(self.nodes.tag_find) {
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
        // With words in the find box, every tag with them in it, however many.
        let find = ui.text(self.nodes.tag_find).trim().to_lowercase();
        let fits = |index: usize, tag: &str| match find.as_str() {
            "" => self.all_tags || index < TAGS_AT_FIRST,
            words => tag.to_lowercase().contains(words),
        };
        let shown: Vec<&(String, usize)> = tags
            .iter()
            .enumerate()
            .filter(|(index, (tag, _))| fits(*index, tag) || picked(tag))
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
        ui.set_visible(self.nodes.tag_find, tags.len() > TAGS_AT_FIRST);
        ui.set_visible(self.nodes.tags_more, find.is_empty() && (self.all_tags || shown.len() < tags.len()));
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
        let pictures = |url: &str| self.picture(&library, open, url);
        let rendered = view::render(ui, self.nodes.preview, &blocks, &Lookups { works: &works, pictures: &pictures });
        self.preview_pictures = pictures_drawn(&library, open, &rendered.pictures);
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
        ui.set_visible(self.nodes.edit, !page.generated || library.type_of(self.open).is_some());

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
        self.progress_line = None;
        if !view::steps_in(&blocks).is_empty() {
            let row = ui.panel(self.nodes.page, voxear_ui::Style::row());
            ui.add_class(row, "cb-progress");
            let line = ui.add_label(row, "");
            ui.add_class(line, "cb-progress-line");
            let reset = ui.button(row, "Reset");
            ui.add_class(reset, "cb-progress-reset");
            self.progress_line = Some((line, reset));
        }
        let open = self.open;
        let works = |url: &str| library.works(url, open);
        let pictures = |url: &str| self.picture(&library, open, url);
        let rendered = view::render(ui, self.nodes.page, &blocks, &Lookups { works: &works, pictures: &pictures });
        self.page_pictures = pictures_drawn(&library, open, &rendered.pictures);
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
        let code = library.code_links(self.open);
        if !code.is_empty() {
            let heading = ui.add_label(self.nodes.page, "In the code");
            ui.add_class(heading, "cb-section");
            for (name, url) in &code {
                let row = ui.panel(self.nodes.page, voxear_ui::Style::row());
                ui.add_class(row, "cb-child");
                let icon = ui.icon(row, "code");
                ui.add_class(icon, "cb-row-icon");
                let mut link = voxear_ui::RichText::new();
                link.push_link(name, url, Default::default());
                ui.rich_label(row, &link);
            }
        }
        self.home_health = None;
        if self.open == library.root() && !library.is_empty() {
            self.home_health = Some(show_home(ui, self.nodes.page, &library));
        }
        drop(library);
        self.show_steps(ui);
        self.show_toc(ui);
    }
    /// Docs Health in the page's place: how many things need looking after,
    /// then each kind, every entry a link to its page or file.
    fn show_health(&mut self, ui: &mut Ui) {
        for child in ui.children(self.nodes.page).into_iter().chain(ui.children(self.nodes.toc)) {
            ui.remove(child);
        }
        (self.toc, self.page_tags, self.rendered) = (Vec::new(), Vec::new(), Rendered::default());
        self.progress_line = None;
        self.page_pictures.clear();
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
        let lost = health.lost_notes.iter().map(|key| {
            let mut text = voxear_ui::RichText::new();
            text.push(key, code);
            text.push(&format!(" · Docs/{}/{key}.md", crate::library::NOTES_FOLDER), Default::default());
            text
        });
        let lost = lost.collect();
        section(
            ui,
            "Notes for code that is gone",
            "Written for types that were renamed, moved or taken out. Rename each file for its type's new name, or \
             delete it.",
            lost,
        );
        let undocumented = health.undocumented.iter().map(|&id| {
            let mut text = voxear_ui::RichText::new();
            page_link(&mut text, id);
            text
        });
        let undocumented = undocumented.collect();
        section(
            ui,
            "Code with no page yet",
            "The project's own types that no page is about. Put [Documentation(\"Page\")] on one to link it to its \
             page. These are not counted among the things to look after.",
            undocumented,
        );
        let gaps = health.comment_gaps.iter().map(|&(id, missing, total)| {
            let mut text = voxear_ui::RichText::new();
            page_link(&mut text, id);
            text.push(&format!(" · {missing} of {total} parts"), Default::default());
            text
        });
        let gaps = gaps.collect();
        section(
            ui,
            "Comments still to write",
            "The project's own types, with how many of their parts have no /// comment, the type itself among them. \
             Not counted either.",
            gaps,
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

/// The pictures a page shows, and the assets it shows live, by their
/// extensions.
const PICTURES: [&str; 4] = ["png", "jpg", "jpeg", "gif"];
const LIVE: [&str; 4] = ["prefab", "material", "gltf", "glb"];

/// The files of the pictures drawn, with where each shows.
fn pictures_drawn(library: &Library, page: PageId, drawn: &[(String, NodeId)]) -> Vec<(PathBuf, NodeId)> {
    drawn.iter().filter_map(|(url, node)| Some((library.picture_path(url, page)?, *node))).collect()
}

/// Folders the Image… menu does not look in: builds, caches and code output.
const NOT_PICTURES: [&str; 5] = ["Builds", "bin", "obj", "target", "node_modules"];

/// Every picture and live asset under `folder`, by its path from `project`.
fn shown_files(project: &Path, folder: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || NOT_PICTURES.contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            shown_files(project, &path, found);
            continue;
        }
        let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let shown = PICTURES.contains(&extension.as_str()) || LIVE.contains(&extension.as_str());
        if let (true, Ok(within)) = (shown, path.strip_prefix(project)) {
            found.push(within.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Markdown that shows only on a line of its own, a picture or a guide's
/// step, with a blank line before and after it, wherever the cursor is.
fn alone(markdown: &str) -> String {
    format!("\n\n{markdown}\n\n")
}

/// A path as a link writes it: in angle brackets when it has spaces.
fn address(path: &str) -> String {
    if path.contains(' ') { format!("<{path}>") } else { path.to_string() }
}

/// Copies a file into `folder` under its own name, or the name with a number
/// when a different file has it there already. Returns the name it took.
fn copy_into(file: &Path, folder: &Path) -> std::io::Result<String> {
    let stem = file.file_stem().unwrap_or_default().to_string_lossy();
    let extension = file.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    keep_into(folder, &stem, &extension, &std::fs::read(file)?)
}

/// Writes `bytes` into `folder` as `stem.extension`, or the name with a
/// number when a different file has it there already; the same file there
/// already is kept. Returns the name it took.
fn keep_into(folder: &Path, stem: &str, extension: &str, bytes: &[u8]) -> std::io::Result<String> {
    std::fs::create_dir_all(folder)?;
    let extension = if extension.is_empty() { String::new() } else { format!(".{extension}") };
    for number in 1.. {
        let name = if number == 1 { format!("{stem}{extension}") } else { format!("{stem} {number}{extension}") };
        let target = folder.join(&name);
        match std::fs::read(&target) {
            Ok(there) if there == bytes => return Ok(name),
            Ok(_) => continue,
            Err(_) => {
                std::fs::write(&target, bytes)?;
                return Ok(name);
            }
        }
    }
    unreachable!("a free name is found")
}

/// What a page being written asks the editor to make for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// A picture of the Scene view.
    Scene,
    /// A few seconds of the Scene view, or the Game view, as a GIF.
    SceneRecording,
    GameRecording,
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
