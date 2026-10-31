//! The Project panel, laid out as Unity's two-column Project window: the
//! project's folders as a tree on the left, and what is in the open folder
//! on the right, its folders first. With room, the right side shows tiles;
//! narrower, it shows rows with each file's size. Selecting a folder in the
//! tree opens it, as does double-clicking one on the right. Assets on the
//! right can be dragged onto the Inspector's asset fields.
//!
//! Selecting an item shows it in the Inspector, and double-clicking a file
//! opens it, through [`ProjectEvent`]s. A context click on an item, on the
//! space around them, or on a folder in the tree offers Open, Show in Files,
//! Copy Path, Rename, Duplicate and Delete, and new folders, scenes, levels,
//! materials and C# scripts (see `project_files`). Rename, and anything new,
//! opens a bar over the files to type the name in: Enter keeps it, and going
//! elsewhere leaves the old one.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use voxear_assets::{AssetDb, AssetKind};
use voxear_ui::{DragSource, ListItems, MenuItem, NodeId, Style, TreeItems, Ui};

use crate::project_files::{self, NewFile};

/// An item of the project, as the panel tells the editor about it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectItem {
    /// From the project's folder; empty for the project's own folder.
    pub path: String,
    pub name: String,
    pub kind: ItemKind,
    /// In bytes; 0 for a folder.
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemKind {
    Folder,
    Script,
    Asset(AssetKind),
}

/// What the panel asks of the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectEvent {
    /// An item was selected, to show in the Inspector.
    Selected(ProjectItem),
    /// A file was opened: double-clicked, or Open from its menu.
    Open(ProjectItem),
    /// Files changed on disk: the editor looks at the assets again and calls
    /// [`ProjectView::refresh`], selecting `select`, and with `rename`,
    /// starting to rename it.
    Changed { select: Option<String>, rename: bool },
    /// Something to say in the status bar.
    Notice(String),
    /// Something was dropped on the files, such as an Obj from the Hierarchy
    /// to make a prefab of, here: in `folder` (from the project's folder).
    Dropped { source: DragSource, folder: String },
}

/// What a context menu's item does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MenuAction {
    Open,
    Reveal,
    CopyPath,
    Rename,
    Duplicate,
    Delete,
    New(NewFile),
}

/// A tile's width with the gap after it, and a row of tiles' height.
const TILE: f32 = 72.0;
const TILE_ROW: f32 = 76.0;
/// A file row's height, as `item-height` gives it in Editor.vui.
const ROW: f32 = 24.0;
/// With room for fewer tiles than this, the files show as rows.
const MIN_COLUMNS: usize = 3;

#[derive(Clone, Copy, PartialEq)]
enum FileKind {
    Asset(AssetKind),
    Script,
}

struct Folder {
    name: String,
    /// From the project's folder; empty for the project's own.
    path: String,
    parent: Option<usize>,
    folders: Vec<usize>,
    files: Vec<usize>,
}

struct File {
    name: String,
    /// From the project's folder, as the asset database names it.
    path: String,
    kind: FileKind,
    size: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Entry {
    Folder(usize),
    File(usize),
}

/// The project as folders and files, and what the right side shows.
struct Contents {
    folders: Vec<Folder>,
    files: Vec<File>,
    /// The open folder's folders, then its files.
    listing: Vec<Entry>,
    /// Tiles across a row, or 0 for rows.
    columns: usize,
    selected: Option<Entry>,
    /// Each tile made, with the place in `listing` it shows, for clicks.
    tiles: Vec<(NodeId, Option<usize>)>,
}

impl Contents {
    fn icon(&self, entry: Entry) -> &'static str {
        match entry {
            Entry::Folder(_) => "folder",
            Entry::File(file) => match self.files[file].kind {
                FileKind::Script => "code",
                FileKind::Asset(AssetKind::Level) => "level",
                FileKind::Asset(AssetKind::Scene) => "scene",
                FileKind::Asset(AssetKind::Prefab) => "cube",
                FileKind::Asset(AssetKind::Model) => "box",
                FileKind::Asset(AssetKind::Material) => "material",
                FileKind::Asset(AssetKind::Texture) => "texture",
            },
        }
    }

    fn name(&self, entry: Entry) -> &str {
        match entry {
            Entry::Folder(folder) => &self.folders[folder].name,
            Entry::File(file) => &self.files[file].name,
        }
    }

    fn item(&self, entry: Entry) -> ProjectItem {
        match entry {
            Entry::Folder(at) => {
                let folder = &self.folders[at];
                ProjectItem { path: folder.path.clone(), name: folder.name.clone(), kind: ItemKind::Folder, size: 0 }
            }
            Entry::File(at) => {
                let file = &self.files[at];
                let kind = match file.kind {
                    FileKind::Script => ItemKind::Script,
                    FileKind::Asset(kind) => ItemKind::Asset(kind),
                };
                ProjectItem { path: file.path.clone(), name: file.name.clone(), kind, size: file.size }
            }
        }
    }

    /// How many list rows the listing takes.
    fn rows(&self) -> usize {
        match self.columns {
            0 => self.listing.len(),
            columns => self.listing.len().div_ceil(columns),
        }
    }
}

type Shared = Rc<RefCell<Contents>>;

struct FolderItems(Shared);

impl TreeItems for FolderItems {
    fn children(&self, parent: Option<u64>) -> Vec<u64> {
        match parent {
            None => vec![0],
            Some(parent) => self.0.borrow().folders[parent as usize].folders.iter().map(|&f| f as u64).collect(),
        }
    }

    fn has_children(&self, item: u64) -> bool {
        !self.0.borrow().folders[item as usize].folders.is_empty()
    }

    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        let icon = ui.icon(row, "folder");
        ui.add_class(icon, "row-icon");
        let name = ui.add_label(row, "");
        ui.add_class(name, "row-name");
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let parts = ui.children(row);
        ui.set_text(parts[1], &self.0.borrow().folders[item as usize].name);
    }
}

/// The open folder's entries as rows: an icon, the name, and a file's size.
struct FileRows(Shared);

impl ListItems for FileRows {
    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        let icon = ui.icon(row, "file");
        ui.add_class(icon, "row-icon");
        let name = ui.add_label(row, "");
        ui.add_class(name, "row-name");
        let size = ui.add_label(row, "");
        ui.add_class(size, "file-size");
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, index: usize) {
        let contents = self.0.borrow();
        let Some(&entry) = contents.listing.get(index) else { return };
        let parts = ui.children(row);
        ui.set_icon(parts[0], contents.icon(entry));
        ui.set_text(parts[1], contents.name(entry));
        let size = match entry {
            Entry::Folder(_) => String::new(),
            Entry::File(file) => file_size(contents.files[file].size),
        };
        ui.set_text(parts[2], &size);
    }
}

/// The open folder's entries as rows of tiles: a big icon over the name.
struct TileRows(Shared);

impl ListItems for TileRows {
    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        ui.add_class(row, "tile-row");
        let columns = self.0.borrow().columns;
        for _ in 0..columns {
            let tile = ui.panel(row, Style::column());
            ui.add_class(tile, "tile");
            ui.set_draggable(tile, true);
            let icon = ui.icon(tile, "file");
            ui.add_class(icon, "tile-icon");
            let name = ui.add_label(tile, "");
            ui.add_class(name, "tile-name");
            self.0.borrow_mut().tiles.push((tile, None));
        }
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, index: usize) {
        let mut contents = self.0.borrow_mut();
        let tiles: Vec<NodeId> = ui.children(row).into_iter().filter(|&n| ui.has_class(n, "tile")).collect();
        for (column, tile) in tiles.into_iter().enumerate() {
            let at = index * contents.columns + column;
            let entry = contents.listing.get(at).copied();
            if let Some(slot) = contents.tiles.iter_mut().find(|(node, _)| *node == tile) {
                slot.1 = entry.map(|_| at);
            }
            ui.set_visible(tile, entry.is_some());
            let Some(entry) = entry else { continue };
            let parts = ui.children(tile);
            ui.set_icon(parts[0], contents.icon(entry));
            ui.set_text(parts[1], contents.name(entry));
            ui.set_checked(tile, contents.selected == Some(entry));
        }
    }
}

pub struct ProjectView {
    folder_tree: NodeId,
    files: NodeId,
    path: NodeId,
    count: NodeId,
    contents: Shared,
    open: usize,
    /// The project's folder and name, for its files and the tree's top, and
    /// the namespace new scripts go in.
    root: PathBuf,
    name: String,
    namespace: String,
    /// The rename bar and its field, and what is being renamed.
    rename_bar: NodeId,
    rename_field: NodeId,
    renaming: Option<String>,
    /// The rename bar was just shown; its field takes focus once it can.
    focus_rename: bool,
    /// The open context menu: what it was opened on (nothing for the open
    /// folder), and what each of its items does.
    menu: Option<(NodeId, Option<Entry>, Vec<Option<MenuAction>>)>,
}

impl ProjectView {
    /// Fills the Project panel in `document` with the assets in `db` and the
    /// scripts under `root`, opening the top folder, named `name`.
    pub fn new(ui: &mut Ui, document: NodeId, name: &str, root: &Path, db: &AssetDb, namespace: &str) -> Self {
        let find = |ui: &Ui, part: &str| ui.find(document, part).unwrap_or_else(|| panic!("Editor.vui has no {part}"));
        let (folder_tree, files) = (find(ui, "project-folders"), find(ui, "project-files"));
        let (path, count) = (find(ui, "project-path"), find(ui, "project-count"));
        let (rename_bar, rename_field) = (find(ui, "project-rename"), find(ui, "project-rename-field"));
        ui.set_visible(rename_bar, false);
        let contents = Rc::new(RefCell::new(scan(name, root, db)));
        ui.bind_tree(folder_tree, FolderItems(contents.clone()));
        ui.set_expanded(folder_tree, 0, true);
        ui.bind_list(files, 0, FileRows(contents.clone()));
        ui.set_rows_draggable(files, true);
        ui.accept_drops(files, true);
        let mut view = Self {
            folder_tree,
            files,
            path,
            count,
            contents,
            open: usize::MAX,
            root: root.to_path_buf(),
            name: name.to_string(),
            namespace: namespace.to_string(),
            rename_bar,
            rename_field,
            renaming: None,
            focus_rename: false,
            menu: None,
        };
        view.open(ui, 0);
        view
    }

    /// Follows clicks and menus, and shows tiles or rows for the room there
    /// is. Returns what the editor is asked to do.
    pub fn update(&mut self, ui: &mut Ui) -> Vec<ProjectEvent> {
        let mut events = Vec::new();
        // The panel picks folders itself, opening them; only another one
        // picked in the tree is shown in the Inspector.
        if ui.selection_changed(self.folder_tree)
            && let Some(&folder) = ui.selected_items(self.folder_tree).first()
            && folder as usize != self.open
        {
            self.open(ui, folder as usize);
            events.push(ProjectEvent::Selected(self.contents.borrow().item(Entry::Folder(folder as usize))));
        }
        self.fit(ui);
        let columns = self.contents.borrow().columns;
        let picked = if columns == 0 { self.row_clicks(ui) } else { self.tile_clicks(ui) };
        if let Some((entry, twice)) = picked {
            match (twice, entry) {
                (true, Entry::Folder(folder)) => self.open(ui, folder),
                (true, Entry::File(_)) => events.push(ProjectEvent::Open(self.contents.borrow().item(entry))),
                _ if self.contents.borrow().selected != Some(entry) => {
                    self.contents.borrow_mut().selected = Some(entry);
                    ui.refresh_list(self.files);
                    events.push(ProjectEvent::Selected(self.contents.borrow().item(entry)));
                }
                _ => {}
            }
        }
        self.open_menus(ui);
        events.extend(self.menu_choice(ui));
        if let Some(source) = ui.dropped_onto(self.files) {
            let folder = self.contents.borrow().folders[self.open].path.clone();
            events.push(ProjectEvent::Dropped { source, folder });
        }
        if let Some(event) = self.update_rename(ui) {
            events.push(event);
        }
        events
    }

    /// Reads the project's files again, as they are after something changed
    /// them, keeping the open folder, and selects `select` when given.
    pub fn refresh(&mut self, ui: &mut Ui, db: &AssetDb, select: Option<&str>) {
        let open = self.contents.borrow().folders.get(self.open).map(|f| f.path.clone()).unwrap_or_default();
        let mut contents = scan(&self.name, &self.root, db);
        // So the rows are made again for the new contents, whichever kind fits.
        contents.columns = usize::MAX;
        *self.contents.borrow_mut() = contents;
        let folder = self.contents.borrow().folders.iter().position(|f| f.path == open).unwrap_or(0);
        ui.refresh_list(self.folder_tree);
        self.fit(ui);
        self.open = usize::MAX;
        self.open(ui, folder);
        if let Some(path) = select {
            self.reveal(ui, path);
        }
    }

    /// The file or folder at a path, from the project's folder.
    pub fn item(&self, path: &str) -> Option<ProjectItem> {
        let contents = self.contents.borrow();
        let file = contents.files.iter().position(|f| f.path == path).map(Entry::File);
        let entry = file.or_else(|| contents.folders.iter().position(|f| f.path == path).map(Entry::Folder))?;
        Some(contents.item(entry))
    }

    /// Opens the rename bar on the selected item.
    pub fn start_rename(&mut self, ui: &mut Ui) {
        let Some(entry) = self.contents.borrow().selected else { return };
        let item = self.contents.borrow().item(entry);
        if item.path.is_empty() {
            return;
        }
        ui.set_visible(self.rename_bar, true);
        ui.set_text(self.rename_field, &item.name);
        self.renaming = Some(item.path);
        self.focus_rename = true;
    }

    /// The rename bar: Enter renames, and leaving the field gives up.
    fn update_rename(&mut self, ui: &mut Ui) -> Option<ProjectEvent> {
        let path = self.renaming.clone()?;
        // Shown last frame, so it can take focus now, with its name selected to type over.
        if std::mem::take(&mut self.focus_rename) {
            ui.focus(self.rename_field);
            ui.select_all_text(self.rename_field);
            return None;
        }
        if ui.submitted(self.rename_field) {
            self.end_rename(ui);
            let name = ui.text(self.rename_field);
            return Some(match project_files::rename(&self.root, &path, &name) {
                Ok(renamed) => ProjectEvent::Changed { select: Some(renamed), rename: false },
                Err(error) => ProjectEvent::Notice(format!("{error:#}")),
            });
        }
        if ui.focused() != Some(self.rename_field) {
            self.end_rename(ui);
        }
        None
    }

    fn end_rename(&mut self, ui: &mut Ui) {
        self.renaming = None;
        ui.set_visible(self.rename_bar, false);
    }

    /// Opens a context menu on what was context-clicked: a tile or row, the
    /// space around them, or a folder in the tree.
    fn open_menus(&mut self, ui: &mut Ui) {
        let tile = {
            let contents = self.contents.borrow();
            let clicked = contents.tiles.iter().find(|(tile, at)| at.is_some() && ui.context_clicked(*tile));
            clicked.and_then(|(_, at)| contents.listing.get((*at)?).copied())
        };
        let target = if let Some(entry) = tile {
            self.contents.borrow_mut().selected = Some(entry);
            ui.refresh_list(self.files);
            Some((self.files, Some(entry)))
        } else if ui.context_clicked(self.files) {
            let contents = self.contents.borrow();
            let row = ui.selection(self.files).first().and_then(|&at| contents.listing.get(at).copied());
            let row = row.filter(|_| contents.columns == 0);
            drop(contents);
            self.contents.borrow_mut().selected = row;
            Some((self.files, row))
        } else if ui.context_clicked(self.folder_tree) {
            let folder = ui.selected_items(self.folder_tree).first().map(|&f| f as usize).unwrap_or(self.open);
            Some((self.folder_tree, Some(Entry::Folder(folder))))
        } else {
            None
        };
        let Some((owner, entry)) = target else { return };
        let mut items = Vec::new();
        let mut actions = Vec::new();
        let mut add = |item: MenuItem, action: Option<MenuAction>| {
            items.push(item);
            actions.push(action);
        };
        let is_root = entry == Some(Entry::Folder(0));
        if let Some(entry) = entry {
            add(MenuItem::new("Open"), Some(MenuAction::Open));
            add(MenuItem::new("Show in Files"), Some(MenuAction::Reveal));
            add(MenuItem::new("Copy Path"), Some(MenuAction::CopyPath));
            add(MenuItem::separator(), None);
            add(MenuItem::new("Rename").enabled(!is_root), Some(MenuAction::Rename));
            add(MenuItem::new("Duplicate").enabled(matches!(entry, Entry::File(_))), Some(MenuAction::Duplicate));
            add(MenuItem::new("Delete").enabled(!is_root), Some(MenuAction::Delete));
        } else {
            add(MenuItem::new("Show in Files"), Some(MenuAction::Reveal));
            add(MenuItem::new("Copy Path"), Some(MenuAction::CopyPath));
        }
        add(MenuItem::separator(), None);
        for kind in NewFile::ALL {
            add(MenuItem::new(kind.label()), Some(MenuAction::New(kind)));
        }
        let at = ui.pointer_position().unwrap_or_default();
        ui.open_menu(owner, at, &items);
        self.menu = Some((owner, entry, actions));
    }

    /// Does what was chosen in a context menu.
    fn menu_choice(&mut self, ui: &mut Ui) -> Vec<ProjectEvent> {
        let Some((owner, entry, actions)) = self.menu.as_ref() else { return Vec::new() };
        let Some(choice) = ui.menu_choice(*owner) else { return Vec::new() };
        let (entry, action) = (*entry, actions.get(choice).copied().flatten());
        self.menu = None;
        // Nothing chosen on an item means the open folder.
        let item = self.contents.borrow().item(entry.unwrap_or(Entry::Folder(self.open)));
        let folder = match entry {
            Some(Entry::Folder(folder)) => folder,
            _ => self.open,
        };
        let failed = |error: anyhow::Error| vec![ProjectEvent::Notice(format!("{error:#}"))];
        let Some(action) = action else { return Vec::new() };
        match action {
            MenuAction::Open => match entry {
                Some(Entry::Folder(folder)) => {
                    self.open(ui, folder);
                    Vec::new()
                }
                _ => vec![ProjectEvent::Open(item)],
            },
            MenuAction::Reveal => {
                let shown = if item.kind == ItemKind::Folder { item.path.as_str() } else { parent(&item.path) };
                show_in_files(&self.root.join(shown));
                Vec::new()
            }
            MenuAction::CopyPath => {
                ui.copy_text(&item.path);
                vec![ProjectEvent::Notice(format!("Copied {}", item.path))]
            }
            MenuAction::Rename => {
                self.start_rename(ui);
                Vec::new()
            }
            MenuAction::Duplicate => match project_files::duplicate(&self.root, &item.path) {
                Ok(copy) => vec![ProjectEvent::Changed { select: Some(copy), rename: false }],
                Err(error) => failed(error),
            },
            MenuAction::Delete => match project_files::trash(&self.root, &item.path) {
                Ok(_) => vec![
                    ProjectEvent::Changed { select: None, rename: false },
                    ProjectEvent::Notice(format!("Moved {} to the project's .voxear/Trash", item.name)),
                ],
                Err(error) => failed(error),
            },
            MenuAction::New(kind) => {
                let at = self.contents.borrow().folders[folder].path.clone();
                match project_files::create(&self.root, &at, kind, &self.namespace) {
                    Ok(made) => vec![ProjectEvent::Changed { select: Some(made), rename: true }],
                    Err(error) => failed(error),
                }
            }
        }
    }

    /// Opens the folder a file is in and selects the file, as a link to it
    /// from the Cookbook does. `path` is from the project's folder.
    pub fn reveal(&mut self, ui: &mut Ui, path: &str) {
        let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let name = parts.pop().unwrap_or_default();
        let mut folder = 0;
        for part in parts {
            let contents = self.contents.borrow();
            let Some(&child) = contents.folders[folder].folders.iter().find(|&&f| contents.folders[f].name == part)
            else {
                return;
            };
            drop(contents);
            folder = child;
        }
        self.open(ui, folder);
        let contents = self.contents.borrow();
        let found = contents.listing.iter().position(|e| contents.name(*e) == name);
        let entry = found.map(|index| (index, contents.listing[index]));
        let columns = contents.columns;
        drop(contents);
        let Some((index, entry)) = entry else { return };
        self.contents.borrow_mut().selected = Some(entry);
        if columns == 0 {
            ui.set_selection(self.files, &[index]);
        }
        ui.scroll_to_index(self.files, index.checked_div(columns).unwrap_or(index));
        ui.refresh_list(self.files);
    }

    /// Shows what is in `folder`, and marks it in the tree.
    fn open(&mut self, ui: &mut Ui, folder: usize) {
        if folder == self.open {
            return;
        }
        self.open = folder;
        let mut contents = self.contents.borrow_mut();
        let listing: Vec<Entry> = {
            let at = &contents.folders[folder];
            at.folders.iter().map(|&f| Entry::Folder(f)).chain(at.files.iter().map(|&f| Entry::File(f))).collect()
        };
        contents.listing = listing;
        contents.selected = None;
        let mut crumbs = vec![contents.folders[folder].name.clone()];
        let mut parent = contents.folders[folder].parent;
        while let Some(at) = parent {
            crumbs.push(contents.folders[at].name.clone());
            parent = contents.folders[at].parent;
            ui.set_expanded(self.folder_tree, at as u64, true);
        }
        crumbs.reverse();
        ui.set_text(self.path, &crumbs.join("  /  "));
        let items = contents.listing.len();
        ui.set_text(self.count, &format!("{items} {}", if items == 1 { "item" } else { "items" }));
        let rows = contents.rows();
        drop(contents);
        ui.set_list_count(self.files, rows);
        ui.set_selection(self.files, &[]);
        ui.refresh_list(self.files);
        ui.scroll_to_index(self.files, 0);
        if ui.selected_items(self.folder_tree).first() != Some(&(folder as u64)) {
            ui.select_item(self.folder_tree, folder as u64);
        }
    }

    /// Tiles when at least a few fit across, rows otherwise, rebuilding the
    /// list's rows when that changes.
    fn fit(&mut self, ui: &mut Ui) {
        let width = ui.rect(self.files)[2];
        let fits = ((width - 12.0) / TILE).floor().max(0.0) as usize;
        let columns = if fits >= MIN_COLUMNS { fits } else { 0 };
        if columns == self.contents.borrow().columns {
            return;
        }
        {
            let mut contents = self.contents.borrow_mut();
            contents.columns = columns;
            contents.tiles.clear();
        }
        let rows = self.contents.borrow().rows();
        // Rows of files drag one by one; a row of tiles does not, its tiles do.
        ui.set_rows_draggable(self.files, columns == 0);
        if columns == 0 {
            ui.set_item_height(self.files, ROW);
            ui.bind_list(self.files, rows, FileRows(self.contents.clone()));
        } else {
            ui.set_item_height(self.files, TILE_ROW);
            ui.bind_list(self.files, rows, TileRows(self.contents.clone()));
        }
    }

    /// The project path of the asset being dragged, when it is one of this
    /// view's: a tile or a row of the open folder's files. Not a folder or a
    /// script, which no field takes.
    pub fn dragged(&self, source: &DragSource) -> Option<String> {
        let contents = self.contents.borrow();
        let at = match *source {
            DragSource::Row { list, index, .. } if list == self.files && contents.columns == 0 => index,
            DragSource::Element(tile) => contents.tiles.iter().find(|(node, _)| *node == tile)?.1?,
            _ => return None,
        };
        let Entry::File(file) = *contents.listing.get(at)? else { return None };
        let file = &contents.files[file];
        matches!(file.kind, FileKind::Asset(_)).then(|| file.path.clone())
    }

    /// A row picked in the list, and whether it was a double click.
    fn row_clicks(&self, ui: &Ui) -> Option<(Entry, bool)> {
        let index = *ui.selection(self.files).first()?;
        let entry = *self.contents.borrow().listing.get(index)?;
        let twice = ui.double_clicked(self.files);
        (ui.selection_changed(self.files) || twice).then_some((entry, twice))
    }

    /// A tile clicked, and whether it was a double click.
    fn tile_clicks(&self, ui: &Ui) -> Option<(Entry, bool)> {
        let contents = self.contents.borrow();
        let (tile, at) = contents.tiles.iter().find(|(tile, at)| at.is_some() && ui.clicked(*tile))?;
        let entry = *contents.listing.get((*at)?)?;
        Some((entry, ui.double_clicked(*tile)))
    }
}

/// The project's folders and files: its assets, its C# scripts, and every
/// folder, empty ones too, leaving out build output and hidden folders such
/// as `.voxear`.
fn scan(name: &str, root: &Path, db: &AssetDb) -> Contents {
    let top =
        Folder { name: name.to_string(), path: String::new(), parent: None, folders: Vec::new(), files: Vec::new() };
    let mut contents = Contents {
        folders: vec![top],
        files: Vec::new(),
        listing: Vec::new(),
        columns: 0,
        selected: None,
        tiles: Vec::new(),
    };
    let mut paths: Vec<(String, FileKind)> = db.entries().map(|e| (e.path.clone(), FileKind::Asset(e.kind))).collect();
    let mut folders = Vec::new();
    scripts(root, root, &mut paths, &mut folders);
    folders.sort_by_key(|path| path.to_lowercase());
    for path in folders {
        let mut folder = 0;
        for part in path.split('/') {
            folder = child_folder(&mut contents, folder, part);
        }
    }
    paths.sort_by_key(|(path, _)| path.to_lowercase());
    for (path, kind) in paths {
        let size = std::fs::metadata(root.join(&path)).map_or(0, |m| m.len());
        let mut parts: Vec<&str> = path.split('/').collect();
        let file_name = parts.pop().unwrap_or_default().to_string();
        let mut folder = 0;
        for part in parts {
            folder = child_folder(&mut contents, folder, part);
        }
        contents.files.push(File { name: file_name, path: path.clone(), kind, size });
        let file = contents.files.len() - 1;
        contents.folders[folder].files.push(file);
    }
    contents
}

/// The folder called `name` in `parent`, made if it is not there yet.
fn child_folder(contents: &mut Contents, parent: usize, name: &str) -> usize {
    let found = contents.folders[parent].folders.iter().copied().find(|&f| contents.folders[f].name == name);
    found.unwrap_or_else(|| {
        let at = &contents.folders[parent].path;
        let path = if at.is_empty() { name.to_string() } else { format!("{at}/{name}") };
        contents.folders.push(Folder {
            name: name.to_string(),
            path,
            parent: Some(parent),
            folders: Vec::new(),
            files: Vec::new(),
        });
        let folder = contents.folders.len() - 1;
        contents.folders[parent].folders.push(folder);
        folder
    })
}

/// C# scripts and folders under `dir`, relative to `root`, leaving out build
/// output and hidden folders such as `.voxear`.
fn scripts(root: &Path, dir: &Path, found: &mut Vec<(String, FileKind)>, folders: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') && name != "bin" && name != "obj" {
                if let Ok(relative) = path.strip_prefix(root) {
                    folders.push(relative.to_string_lossy().replace('\\', "/"));
                }
                scripts(root, &path, found, folders);
            }
        } else if name.ends_with(".cs")
            && let Ok(relative) = path.strip_prefix(root)
        {
            found.push((relative.to_string_lossy().replace('\\', "/"), FileKind::Script));
        }
    }
}

/// A size as file managers write it.
fn file_size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// The folder a path is in, from the project's folder.
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// Opens a folder in the system's file manager.
fn show_in_files(folder: &Path) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    match std::process::Command::new(program).arg(folder).spawn() {
        // Waited on elsewhere, so it does not linger once it ends.
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(error) => tracing::warn!("could not open {}: {error}", folder.display()),
    }
}
