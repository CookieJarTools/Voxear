//! The Project panel, laid out as Unity's two-column Project window: the
//! project's folders as a tree on the left, and what is in the open folder
//! on the right, its folders first. With room, the right side shows tiles;
//! narrower, it shows rows with each file's size. Selecting a folder in the
//! tree opens it, as does double-clicking one on the right.

use std::{cell::RefCell, path::Path, rc::Rc};

use voxear_assets::{AssetDb, AssetKind};
use voxear_ui::{ListItems, NodeId, Style, TreeItems, Ui};

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
    parent: Option<usize>,
    folders: Vec<usize>,
    files: Vec<usize>,
}

struct File {
    name: String,
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
}

impl ProjectView {
    /// Fills the Project panel in `document` with the assets in `db` and the
    /// scripts under `root`, opening the top folder, named `name`.
    pub fn new(ui: &mut Ui, document: NodeId, name: &str, root: &Path, db: &AssetDb) -> Self {
        let find = |ui: &Ui, part: &str| ui.find(document, part).unwrap_or_else(|| panic!("Editor.vui has no {part}"));
        let (folder_tree, files) = (find(ui, "project-folders"), find(ui, "project-files"));
        let (path, count) = (find(ui, "project-path"), find(ui, "project-count"));
        let contents = Rc::new(RefCell::new(scan(name, root, db)));
        ui.bind_tree(folder_tree, FolderItems(contents.clone()));
        ui.set_expanded(folder_tree, 0, true);
        ui.bind_list(files, 0, FileRows(contents.clone()));
        let mut view = Self { folder_tree, files, path, count, contents, open: usize::MAX };
        view.open(ui, 0);
        view
    }

    /// Follows clicks, and shows tiles or rows for the room there is.
    pub fn update(&mut self, ui: &mut Ui) {
        if ui.selection_changed(self.folder_tree)
            && let Some(&folder) = ui.selected_items(self.folder_tree).first()
        {
            self.open(ui, folder as usize);
        }
        self.fit(ui);
        let columns = self.contents.borrow().columns;
        let picked = if columns == 0 { self.row_clicks(ui) } else { self.tile_clicks(ui) };
        if let Some((entry, twice)) = picked {
            if let (true, Entry::Folder(folder)) = (twice, entry) {
                self.open(ui, folder);
            } else if self.contents.borrow().selected != Some(entry) {
                self.contents.borrow_mut().selected = Some(entry);
                ui.refresh_list(self.files);
            }
        }
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
        if columns == 0 {
            ui.set_item_height(self.files, ROW);
            ui.bind_list(self.files, rows, FileRows(self.contents.clone()));
        } else {
            ui.set_item_height(self.files, TILE_ROW);
            ui.bind_list(self.files, rows, TileRows(self.contents.clone()));
        }
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

/// The project's folders and files: its assets, and its C# scripts.
fn scan(name: &str, root: &Path, db: &AssetDb) -> Contents {
    let mut contents = Contents {
        folders: vec![Folder { name: name.to_string(), parent: None, folders: Vec::new(), files: Vec::new() }],
        files: Vec::new(),
        listing: Vec::new(),
        columns: 0,
        selected: None,
        tiles: Vec::new(),
    };
    let mut paths: Vec<(String, FileKind)> = db.entries().map(|e| (e.path.clone(), FileKind::Asset(e.kind))).collect();
    scripts(root, root, &mut paths);
    paths.sort_by_key(|(path, _)| path.to_lowercase());
    for (path, kind) in paths {
        let size = std::fs::metadata(root.join(&path)).map_or(0, |m| m.len());
        let mut parts: Vec<&str> = path.split('/').collect();
        let file_name = parts.pop().unwrap_or_default().to_string();
        let mut folder = 0;
        for part in parts {
            folder = child_folder(&mut contents, folder, part);
        }
        contents.files.push(File { name: file_name, kind, size });
        let file = contents.files.len() - 1;
        contents.folders[folder].files.push(file);
    }
    contents
}

/// The folder called `name` in `parent`, made if it is not there yet.
fn child_folder(contents: &mut Contents, parent: usize, name: &str) -> usize {
    let found = contents.folders[parent].folders.iter().copied().find(|&f| contents.folders[f].name == name);
    found.unwrap_or_else(|| {
        contents.folders.push(Folder {
            name: name.to_string(),
            parent: Some(parent),
            folders: Vec::new(),
            files: Vec::new(),
        });
        let folder = contents.folders.len() - 1;
        contents.folders[parent].folders.push(folder);
        folder
    })
}

/// C# scripts under `dir`, relative to `root`, leaving out build output and
/// hidden folders such as `.voxear`.
fn scripts(root: &Path, dir: &Path, found: &mut Vec<(String, FileKind)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') && name != "bin" && name != "obj" {
                scripts(root, &path, found);
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
