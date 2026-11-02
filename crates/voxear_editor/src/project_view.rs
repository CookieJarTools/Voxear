//! The Project panel, laid out as Unity's two-column Project window: the
//! project's folders as a tree on the left, and what is in the open folder
//! on the right, its folders first, as tiles, or as rows with each file's
//! size at the smallest size. Selecting a folder in the
//! tree opens it, as does double-clicking one on the right. Assets on the
//! right can be dragged onto the Inspector's asset fields, and prefabs,
//! models and scenes onto the Hierarchy and the Scene view. An Obj dragged
//! from the Hierarchy onto the files makes a prefab in the open folder, or
//! onto a folder's tile or a folder in the tree, in that folder.
//!
//! Files and folders dragged onto a folder, its tile or row or its place in
//! the tree, move into it with their `.meta`, so references to them keep
//! working; onto a folder higher up the tree, they move out. A slider over
//! the files sets how big the tiles are, the smallest being rows, as in
//! Unity's Project window.
//!
//! Selecting an item shows it in the Inspector, and double-clicking a file
//! opens it, through [`ProjectEvent`]s. A context click on an item, on the
//! space around them, or on a folder in the tree offers Open (and for a
//! scene, Open Additively, alongside the scenes open), Show in Files, Copy
//! Path, Rename, Duplicate and Delete, and new folders, scenes, levels,
//! materials and C# scripts (see `project_files`). Rename, and anything new,
//! opens a bar over the files to type the name in: Enter keeps it, and going
//! elsewhere leaves the old one.
//!
//! Materials, prefabs and models show previews of themselves in place of
//! their icons once the editor has them drawn (see [`ProjectView::set_previews`]).
//!
//! A model has an arrow that opens it out, as Unity's Project window does,
//! to show its meshes and materials after it, which the editor reads from
//! the model when it is first opened out ([`ProjectEvent::WantsParts`]).
//! A part can be dragged onto a field that takes one, as a mesh onto a Mesh
//! Renderer's Mesh. With the setting on, a prefab opens out to its Objs.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
};

use voxear_assets::{AssetDb, AssetKind};
use voxear_ui::{DragSource, ImageRef, ListItems, MenuItem, NodeId, Style, TreeItems, Ui};

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
    /// A scene was opened alongside those open, from its menu.
    OpenAdditively(ProjectItem),
    /// Files changed on disk: the editor looks at the assets again and calls
    /// [`ProjectView::refresh`], selecting `select`, and with `rename`,
    /// starting to rename it.
    Changed { select: Option<String>, rename: bool },
    /// Something to say in the status bar.
    Notice(String),
    /// Something was dropped on the files, a folder's tile or a folder in
    /// the tree, such as an Obj from the Hierarchy to make a prefab of, in
    /// `folder` (from the project's folder).
    Dropped { source: DragSource, folder: String },
    /// A model was opened out before its parts were known: the editor reads
    /// them and hands them to [`ProjectView::set_parts`].
    WantsParts(String),
    /// Import New Asset was chosen: the editor asks for files to copy into
    /// `folder` (from the project's folder).
    Import { folder: String },
    /// A file or folder of the panel's was dragged onto a folder, by their
    /// paths, to move it there.
    Move { path: String, folder: String },
}

/// A part of a model, or an Obj of a prefab, shown after it when it is
/// opened out.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetPart {
    pub name: String,
    pub icon: &'static str,
    /// What a field takes for it, as `Models/Crate.gltf#mesh0.0`, or `None`
    /// for an Obj inside a prefab, which no field takes on its own.
    pub reference: Option<String>,
}

/// What a context menu's item does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MenuAction {
    Open,
    OpenAdditively,
    Reveal,
    CopyPath,
    Rename,
    Duplicate,
    Delete,
    Import,
    New(NewFile),
}

/// How long a pinged file stays lit, in seconds.
const PING: f32 = 1.5;
/// What a tile takes across beside its picture, with the gap after it, and
/// down beside it, for its name.
const TILE_SIDES: f32 = 28.0;
const TILE_BELOW: f32 = 32.0;
/// A file row's height, as `item-height` gives it in Editor.vui.
const ROW: f32 = 24.0;
/// How big a preview shows in a row, as Editor.vss sizes `row-thumb`.
const ROW_PREVIEW: f32 = 16.0;
/// The size slider's value below which the files show as rows, and the
/// smallest and largest a tile's picture is past it, in logical pixels.
const ROWS_BELOW: f32 = 0.05;
const TILE_PICTURES: (f32, f32) = (36.0, 128.0);

/// How big a tile's picture is at the size slider's `value`, or `None` for
/// rows.
fn tile_picture(value: f32) -> Option<f32> {
    let (least, most) = TILE_PICTURES;
    let along = (value - ROWS_BELOW) / (1.0 - ROWS_BELOW);
    (value >= ROWS_BELOW).then(|| (least + along.clamp(0.0, 1.0) * (most - least)).round())
}

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
    /// A part of an opened-out file: the file, and which part.
    Part(usize, usize),
}

/// The project as folders and files, and what the right side shows.
struct Contents {
    folders: Vec<Folder>,
    files: Vec<File>,
    /// The open folder's folders, then its files.
    listing: Vec<Entry>,
    /// Tiles across a row, or 0 for rows, and how big a tile's picture is.
    columns: usize,
    picture: f32,
    selected: Option<Entry>,
    /// The entry lit by a ping, as a log line about it asks.
    pinged: Option<Entry>,
    /// Each tile made, with the place in `listing` it shows, for clicks.
    tiles: Vec<(NodeId, Option<usize>)>,
    /// Previews drawn so far, by path, shown in place of icons.
    previews: HashMap<String, ImageRef>,
    /// Each model's or prefab's parts, by its path, once known.
    parts: HashMap<String, Vec<AssetPart>>,
    /// The models and prefabs opened out, by path.
    opened: HashSet<String>,
    /// Whether prefabs open out to their Objs, as a setting says.
    prefab_parts: bool,
    /// Each opening arrow made, with the place in `listing` it opens.
    arrows: Vec<(NodeId, Option<usize>)>,
}

impl Contents {
    fn icon(&self, entry: Entry) -> &'static str {
        match entry {
            Entry::Folder(_) => "folder",
            Entry::Part(file, part) => self.part(file, part).map_or("file", |p| p.icon),
            Entry::File(file) => match self.files[file].kind {
                FileKind::Script => "code",
                FileKind::Asset(AssetKind::Level) => "level",
                FileKind::Asset(AssetKind::Scene) => "scene",
                FileKind::Asset(AssetKind::Prefab) => "cube",
                FileKind::Asset(AssetKind::Model) => "box",
                FileKind::Asset(AssetKind::Material) => "material",
                FileKind::Asset(AssetKind::Texture) => "texture",
                FileKind::Asset(AssetKind::Ui) => "panels",
                FileKind::Asset(AssetKind::Sound) => "audio",
                FileKind::Asset(AssetKind::InputActions) => "gamepad",
            },
        }
    }

    /// What an entry shows in its thumbnail: its preview once drawn, or
    /// else its icon.
    fn show_thumb(&self, ui: &mut Ui, thumb: NodeId, entry: Entry) {
        let parts = ui.children(thumb);
        let preview = match entry {
            Entry::File(file) => self.previews.get(&self.files[file].path).copied(),
            Entry::Folder(_) | Entry::Part(..) => None,
        };
        ui.set_icon(parts[0], self.icon(entry));
        ui.set_visible(parts[0], preview.is_none());
        ui.set_image(parts[1], preview);
        ui.set_visible(parts[1], preview.is_some());
    }

    fn name(&self, entry: Entry) -> &str {
        match entry {
            Entry::Folder(folder) => &self.folders[folder].name,
            Entry::File(file) => &self.files[file].name,
            Entry::Part(file, part) => self.part(file, part).map_or("", |p| &p.name),
        }
    }

    fn part(&self, file: usize, part: usize) -> Option<&AssetPart> {
        self.parts.get(&self.files[file].path)?.get(part)
    }

    /// Whether a file opens out: a model, or a prefab while the setting says.
    fn opens(&self, file: usize) -> bool {
        match self.files[file].kind {
            FileKind::Asset(AssetKind::Model) => true,
            FileKind::Asset(AssetKind::Prefab) => self.prefab_parts,
            _ => false,
        }
    }

    /// What `folder` shows: its folders, then its files, each opened out
    /// one followed by its parts.
    fn list(&self, folder: usize) -> Vec<Entry> {
        let at = &self.folders[folder];
        let mut listing: Vec<Entry> = at.folders.iter().map(|&f| Entry::Folder(f)).collect();
        for &file in &at.files {
            listing.push(Entry::File(file));
            let path = &self.files[file].path;
            if self.opens(file) && self.opened.contains(path) {
                let parts = self.parts.get(path).map_or(0, Vec::len);
                listing.extend((0..parts).map(|part| Entry::Part(file, part)));
            }
        }
        listing
    }

    /// What an entry is, as the Inspector shows it: a part shows the file
    /// it is part of.
    fn item(&self, entry: Entry) -> ProjectItem {
        match entry {
            Entry::Part(file, _) => self.item(Entry::File(file)),
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

    /// Shows an entry's opening arrow, pointing down while it is opened
    /// out, and notes the place in the listing it opens.
    fn bind_arrow(&mut self, ui: &mut Ui, arrow: NodeId, entry: Entry, at: usize) {
        let opens = match entry {
            Entry::File(file) => self.opens(file).then(|| self.opened.contains(&self.files[file].path)),
            _ => None,
        };
        // A row keeps its place when there is nothing to open, so that each row's size lines up; a tile hides it.
        let tile = ui.parent(arrow).is_some_and(|parent| ui.has_class(parent, "tile"));
        ui.set_visible(arrow, opens.is_some() || !tile);
        ui.set_class(arrow, "unused", opens.is_none());
        if let Some(&icon) = ui.children(arrow).first() {
            ui.set_icon(icon, if opens == Some(true) { "chevron-down" } else { "chevron-right" });
        }
        if let Some(slot) = self.arrows.iter_mut().find(|(node, _)| *node == arrow) {
            slot.1 = opens.map(|_| at);
        }
    }

    /// The project path of a file or folder, but not of the project's own
    /// folder or of a part of a file.
    fn entry_path(&self, entry: Entry) -> Option<String> {
        match entry {
            Entry::Folder(folder) => Some(self.folders[folder].path.clone()).filter(|path| !path.is_empty()),
            Entry::File(file) => Some(self.files[file].path.clone()),
            Entry::Part(..) => None,
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

/// The folders' tree rows: each row's content, and the folder it shows.
type FolderRows = Rc<RefCell<Vec<(NodeId, u64)>>>;

struct FolderItems(Shared, FolderRows);

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
        ui.add_class(row, "folder-row");
        let icon = ui.icon(row, "folder");
        ui.add_class(icon, "row-icon");
        let name = ui.add_label(row, "");
        ui.add_class(name, "row-name");
        self.1.borrow_mut().push((row, 0));
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let parts = ui.children(row);
        ui.set_text(parts[1], &self.0.borrow().folders[item as usize].name);
        if let Some(slot) = self.1.borrow_mut().iter_mut().find(|(node, _)| *node == row) {
            slot.1 = item;
        }
    }
}

/// An icon, or the preview in its place, in a box of its own. Returns the
/// box and the icon.
fn thumb(ui: &mut Ui, parent: NodeId, class: &str, icon_class: &str) -> (NodeId, NodeId) {
    let thumb = ui.panel(parent, Style::default());
    ui.add_class(thumb, class);
    let icon = ui.icon(thumb, "file");
    ui.add_class(icon, icon_class);
    let image = ui.image(thumb);
    ui.add_class(image, "thumb-image");
    ui.set_visible(image, false);
    (thumb, icon)
}

/// The open folder's entries as rows: an icon or preview, the name, and a
/// file's size.
struct FileRows(Shared);

impl ListItems for FileRows {
    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        thumb(ui, row, "row-thumb", "row-icon");
        let name = ui.add_label(row, "");
        ui.add_class(name, "row-name");
        let size = ui.add_label(row, "");
        ui.add_class(size, "file-size");
        let arrow = ui.icon_button(row, "chevron-right");
        ui.add_class(arrow, "part-arrow");
        self.0.borrow_mut().arrows.push((arrow, None));
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, index: usize) {
        let mut contents = self.0.borrow_mut();
        let Some(&entry) = contents.listing.get(index) else { return };
        let parts = ui.children(row);
        contents.show_thumb(ui, parts[0], entry);
        ui.set_text(parts[1], contents.name(entry));
        let size = match entry {
            Entry::Folder(_) | Entry::Part(..) => String::new(),
            Entry::File(file) => file_size(contents.files[file].size),
        };
        ui.set_text(parts[2], &size);
        ui.set_class(row, "pinged", contents.pinged == Some(entry));
        ui.set_class(row, "part", matches!(entry, Entry::Part(..)));
        contents.bind_arrow(ui, parts[3], entry, index);
    }
}

/// The open folder's entries as rows of tiles: a big icon or preview over
/// the name.
struct TileRows(Shared);

impl ListItems for TileRows {
    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        ui.add_class(row, "tile-row");
        let (columns, picture) = (self.0.borrow().columns, self.0.borrow().picture);
        for _ in 0..columns {
            let tile = ui.panel(row, Style::column());
            ui.add_class(tile, "tile");
            ui.set_draggable(tile, true);
            ui.set_inline_style(tile, &format!("width: {}px", picture + TILE_SIDES - 2.0));
            let (thumb, icon) = thumb(ui, tile, "tile-thumb", "tile-icon");
            ui.set_inline_style(thumb, &format!("width: {picture}px; height: {picture}px"));
            let icon_size = (picture * 0.6).round();
            ui.set_inline_style(icon, &format!("width: {icon_size}px; height: {icon_size}px"));
            let name = ui.add_label(tile, "");
            ui.add_class(name, "tile-name");
            let arrow = ui.icon_button(tile, "chevron-right");
            ui.add_class(arrow, "part-arrow");
            let mut contents = self.0.borrow_mut();
            contents.tiles.push((tile, None));
            contents.arrows.push((arrow, None));
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
            contents.show_thumb(ui, parts[0], entry);
            ui.set_text(parts[1], contents.name(entry));
            ui.set_checked(tile, contents.selected == Some(entry));
            ui.set_class(tile, "pinged", contents.pinged == Some(entry));
            ui.set_class(tile, "part", matches!(entry, Entry::Part(..)));
            contents.bind_arrow(ui, parts[2], entry, at);
        }
    }
}

pub struct ProjectView {
    folder_tree: NodeId,
    folder_rows: FolderRows,
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
    /// How many seconds the pinged entry stays lit.
    ping_left: f32,
    /// The slider for how big the tiles are, the smallest being rows.
    size: NodeId,
}

impl ProjectView {
    /// Fills the Project panel in `document` with the assets in `db` and the
    /// scripts under `root`, opening the top folder, named `name`.
    pub fn new(ui: &mut Ui, document: NodeId, name: &str, root: &Path, db: &AssetDb, namespace: &str) -> Self {
        let find = |ui: &Ui, part: &str| ui.find(document, part).unwrap_or_else(|| panic!("Editor.vui has no {part}"));
        let (folder_tree, files) = (find(ui, "project-folders"), find(ui, "project-files"));
        let (path, count) = (find(ui, "project-path"), find(ui, "project-count"));
        let (rename_bar, rename_field) = (find(ui, "project-rename"), find(ui, "project-rename-field"));
        let size = find(ui, "project-size");
        ui.set_visible(rename_bar, false);
        let contents = Rc::new(RefCell::new(scan(name, root, db)));
        let folder_rows = FolderRows::default();
        ui.bind_tree(folder_tree, FolderItems(contents.clone(), folder_rows.clone()));
        ui.set_expanded(folder_tree, 0, true);
        ui.bind_list(files, 0, FileRows(contents.clone()));
        ui.set_rows_draggable(files, true);
        ui.accept_drops(files, true);
        let mut view = Self {
            folder_tree,
            folder_rows,
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
            ping_left: 0.0,
            size,
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
        let arrow = self.arrow_clicked(ui);
        if let Some(file) = arrow {
            events.extend(self.open_out(ui, file));
        }
        let picked = if columns == 0 { self.row_clicks(ui) } else { self.tile_clicks(ui) };
        if let Some((entry, twice)) = picked.filter(|_| arrow.is_none()) {
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
        if let Some((source, folder)) = self.dropped(ui) {
            events.push(ProjectEvent::Dropped { source, folder });
        }
        events.extend(self.row_moved(ui));
        if let Some(event) = self.update_rename(ui) {
            events.push(event);
        }
        events
    }

    /// Something let go of on the panel, and the folder it was let go of in:
    /// a folder's tile or a folder in the tree, or else the open folder.
    fn dropped(&self, ui: &Ui) -> Option<(DragSource, String)> {
        let contents = self.contents.borrow();
        let on_tile = contents.tiles.iter().find_map(|&(tile, at)| {
            let source = ui.dropped_onto(tile)?;
            match contents.listing.get(at?)? {
                Entry::Folder(folder) => Some((source, *folder)),
                Entry::File(_) | Entry::Part(..) => None,
            }
        });
        let in_tree = || {
            let rows = self.folder_rows.borrow();
            rows.iter().find_map(|&(row, folder)| Some((ui.dropped_onto(row)?, folder as usize)))
        };
        let in_open = || Some((ui.dropped_onto(self.files)?, self.open));
        let (source, folder) = on_tile.or_else(in_tree).or_else(in_open)?;
        Some((source, contents.folders.get(folder)?.path.clone()))
    }

    /// A row of the files dragged onto a folder's row, as rows show them, to
    /// move it there.
    fn row_moved(&self, ui: &Ui) -> Option<ProjectEvent> {
        let (index, Some((onto, _))) = ui.dropped(self.files)? else { return None };
        let contents = self.contents.borrow();
        let (Some(&from), Some(&Entry::Folder(folder))) = (contents.listing.get(index), contents.listing.get(onto))
        else {
            return None;
        };
        let path = contents.entry_path(from).filter(|_| from != Entry::Folder(folder))?;
        Some(ProjectEvent::Move { path, folder: contents.folders[folder].path.clone() })
    }

    /// The folder under `point`, in the UI's units: a folder's tile or a
    /// folder in the tree, or else the folder open, when the point is over
    /// the panel at all. Files dropped from outside the editor go there.
    pub fn folder_at(&self, ui: &Ui, [x, y]: [f32; 2]) -> Option<String> {
        let over = |node: NodeId| {
            let [left, top, width, height] = ui.rect(node);
            ui.is_visible(node) && (left..left + width).contains(&x) && (top..top + height).contains(&y)
        };
        let contents = self.contents.borrow();
        let tile = contents.tiles.iter().find_map(|&(tile, at)| match contents.listing.get(at?)? {
            Entry::Folder(folder) if over(tile) => Some(*folder),
            _ => None,
        });
        let row = || self.folder_rows.borrow().iter().find_map(|&(row, folder)| over(row).then_some(folder as usize));
        let open = || (over(self.files) || over(self.folder_tree)).then_some(self.open);
        let folder = tile.or_else(row).or_else(open)?;
        contents.folders.get(folder).map(|folder| folder.path.clone())
    }

    /// The project path of the folder open, `""` for the project's own.
    pub fn open_folder(&self) -> String {
        self.contents.borrow().folders.get(self.open).map(|folder| folder.path.clone()).unwrap_or_default()
    }

    /// Lets the folders, as tiles and in the tree, take what is dragged onto
    /// them, or stops them: while an Obj is dragged from the Hierarchy, to
    /// make a prefab in the folder it is let go of on.
    pub fn take_drops(&self, ui: &mut Ui, take: bool) {
        let contents = self.contents.borrow();
        for &(tile, at) in &contents.tiles {
            let folder = at.and_then(|at| contents.listing.get(at)).is_some_and(|e| matches!(e, Entry::Folder(_)));
            ui.accept_drops(tile, take && folder);
        }
        for &(row, _) in self.folder_rows.borrow().iter() {
            ui.accept_drops(row, take);
        }
    }

    /// Reads the project's files again, as they are after something changed
    /// them, keeping the open folder, and selects `select` when given.
    pub fn refresh(&mut self, ui: &mut Ui, db: &AssetDb, select: Option<&str>) {
        let open = self.contents.borrow().folders.get(self.open).map(|f| f.path.clone()).unwrap_or_default();
        let mut contents = scan(&self.name, &self.root, db);
        // So the rows are made again for the new contents, whichever kind fits.
        contents.columns = usize::MAX;
        {
            let mut kept = self.contents.borrow_mut();
            contents.opened = std::mem::take(&mut kept.opened);
            contents.parts = std::mem::take(&mut kept.parts);
            contents.prefab_parts = kept.prefab_parts;
        }
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

    /// The open folder's materials, prefabs and models, by path: what can
    /// show a preview, at [`preview_size`](Self::preview_size). As tiles,
    /// only those on screen, so a big folder asks for a screenful at a time.
    pub fn previewable(&self, ui: &Ui) -> Vec<String> {
        let contents = self.contents.borrow();
        let kinds = |kind| matches!(kind, AssetKind::Material | AssetKind::Prefab | AssetKind::Model);
        let shown: Vec<Entry> = if contents.columns == 0 {
            contents.listing.clone()
        } else {
            let on_screen = contents.tiles.iter().filter(|(tile, _)| ui.is_visible(*tile));
            on_screen.filter_map(|(_, at)| contents.listing.get((*at)?).copied()).collect()
        };
        shown
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::File(file) => Some(&contents.files[file]),
                Entry::Folder(_) | Entry::Part(..) => None,
            })
            .filter(|file| matches!(file.kind, FileKind::Asset(kind) if kinds(kind)))
            .map(|file| file.path.clone())
            .collect()
    }

    /// How big a preview shows, in logical pixels: a tile's, or a row's.
    pub fn preview_size(&self) -> f32 {
        let contents = self.contents.borrow();
        if contents.columns == 0 { ROW_PREVIEW } else { contents.picture }
    }

    /// How big the tiles are, from 0 for rows to 1 for the largest, as the
    /// slider over the files sets it.
    pub fn tile_size(&self, ui: &Ui) -> f32 {
        ui.slider_value(self.size)
    }

    pub fn set_tile_size(&mut self, ui: &mut Ui, size: f32) {
        ui.set_slider_value(self.size, size);
    }

    /// Whether the size slider is being dragged.
    pub fn resizing(&self, ui: &Ui) -> bool {
        ui.is_pressed(self.size)
    }

    /// Shows these previews, by path, in place of icons; anything not among
    /// them shows its icon.
    pub fn set_previews(&mut self, ui: &mut Ui, previews: HashMap<String, ImageRef>) {
        if self.contents.borrow().previews == previews {
            return;
        }
        self.contents.borrow_mut().previews = previews;
        ui.refresh_list(self.files);
    }

    /// Opens the rename bar on the selected item, with its name to type
    /// over: a file's without its extension, which it keeps, as in Unity.
    pub fn start_rename(&mut self, ui: &mut Ui) {
        let Some(entry) = self.contents.borrow().selected else { return };
        let item = self.contents.borrow().item(entry);
        if item.path.is_empty() {
            return;
        }
        let name = match item.kind {
            ItemKind::Folder => item.name.as_str(),
            _ => item.name.rsplit_once('.').map_or(item.name.as_str(), |(stem, _)| stem),
        };
        ui.set_visible(self.rename_bar, true);
        ui.set_text(self.rename_field, name);
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
        // A part is not a file of its own, to rename or delete.
        let Some((owner, entry)) = target.filter(|(_, entry)| !matches!(entry, Some(Entry::Part(..)))) else { return };
        let mut items = Vec::new();
        let mut actions = Vec::new();
        let mut add = |item: MenuItem, action: Option<MenuAction>| {
            items.push(item);
            actions.push(action);
        };
        let is_root = entry == Some(Entry::Folder(0));
        if let Some(entry) = entry {
            add(MenuItem::new("Open"), Some(MenuAction::Open));
            let contents = self.contents.borrow();
            let scene =
                matches!(entry, Entry::File(file) if contents.files[file].kind == FileKind::Asset(AssetKind::Scene));
            drop(contents);
            if scene {
                add(MenuItem::new("Open Additively"), Some(MenuAction::OpenAdditively));
            }
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
        add(MenuItem::new("Import New Asset…"), Some(MenuAction::Import));
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
            MenuAction::OpenAdditively => vec![ProjectEvent::OpenAdditively(item)],
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
            MenuAction::Import => vec![ProjectEvent::Import { folder: self.contents.borrow().folders[folder].path.clone() }],
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
    /// from VoxDocs does. `path` is from the project's folder.
    pub fn reveal(&mut self, ui: &mut Ui, path: &str) {
        let Some((index, entry)) = self.open_folder_of(ui, path) else { return };
        let columns = self.contents.borrow().columns;
        self.contents.borrow_mut().selected = Some(entry);
        if columns == 0 {
            ui.set_selection(self.files, &[index]);
        }
        ui.scroll_to_index(self.files, index.checked_div(columns).unwrap_or(index));
        ui.refresh_list(self.files);
    }

    /// Opens the folder a file is in and lights the file for a moment,
    /// leaving what is selected as it is, as a log line about it asks.
    pub fn ping(&mut self, ui: &mut Ui, path: &str) {
        let Some((index, entry)) = self.open_folder_of(ui, path) else { return };
        let columns = self.contents.borrow().columns;
        self.contents.borrow_mut().pinged = Some(entry);
        self.ping_left = PING;
        ui.scroll_to_index(self.files, index.checked_div(columns).unwrap_or(index));
        ui.refresh_list(self.files);
    }

    /// Counts a ping down by a frame's `delta` seconds, and puts its light
    /// out once it is over.
    pub fn fade_ping(&mut self, ui: &mut Ui, delta: f32) {
        if self.ping_left <= 0.0 {
            return;
        }
        self.ping_left -= delta;
        if self.ping_left <= 0.0 {
            self.contents.borrow_mut().pinged = None;
            ui.refresh_list(self.files);
        }
    }

    /// Opens the folder `path` is in, and finds the file there: its place
    /// in the listing, and its entry.
    fn open_folder_of(&mut self, ui: &mut Ui, path: &str) -> Option<(usize, Entry)> {
        let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let name = parts.pop().unwrap_or_default();
        let mut folder = 0;
        for part in parts {
            let contents = self.contents.borrow();
            let child = contents.folders[folder].folders.iter().find(|&&f| contents.folders[f].name == part)?;
            folder = *child;
        }
        self.open(ui, folder);
        let contents = self.contents.borrow();
        let index = contents.listing.iter().position(|e| contents.name(*e) == name)?;
        Some((index, contents.listing[index]))
    }

    /// Shows what is in `folder`, and marks it in the tree.
    fn open(&mut self, ui: &mut Ui, folder: usize) {
        if folder == self.open {
            return;
        }
        self.open = folder;
        let mut contents = self.contents.borrow_mut();
        contents.listing = contents.list(folder);
        contents.selected = None;
        let mut crumbs = vec![contents.folders[folder].name.clone()];
        let (mut parent, mut above) = (contents.folders[folder].parent, Vec::new());
        while let Some(at) = parent {
            crumbs.push(contents.folders[at].name.clone());
            parent = contents.folders[at].parent;
            above.push(at);
        }
        crumbs.reverse();
        ui.set_text(self.path, &crumbs.join("  /  "));
        let items = contents.listing.len();
        ui.set_text(self.count, &format!("{items} {}", if items == 1 { "item" } else { "items" }));
        let rows = contents.rows();
        drop(contents);
        // Expanding asks the tree's rows for their folders, which needs the contents back.
        for at in above {
            ui.set_expanded(self.folder_tree, at as u64, true);
        }
        ui.set_list_count(self.files, rows);
        ui.set_selection(self.files, &[]);
        ui.refresh_list(self.files);
        ui.scroll_to_index(self.files, 0);
        if ui.selected_items(self.folder_tree).first() != Some(&(folder as u64)) {
            ui.select_item(self.folder_tree, folder as u64);
        }
    }

    /// Tiles of the size the slider sets, as many across as fit and smaller
    /// when not even one does, or rows at its smallest, rebuilding the
    /// list's rows when that changes.
    fn fit(&mut self, ui: &mut Ui) {
        let width = ui.rect(self.files)[2];
        let picture = tile_picture(ui.slider_value(self.size))
            .map(|picture| picture.min((width - 12.0 - TILE_SIDES).floor()).max(TILE_PICTURES.0));
        let fits = |picture: f32| ((width - 12.0) / (picture + TILE_SIDES)).floor().max(1.0) as usize;
        let columns = picture.map_or(0, fits);
        let picture = picture.unwrap_or_default();
        if (columns, picture) == (self.contents.borrow().columns, self.contents.borrow().picture) {
            return;
        }
        {
            let mut contents = self.contents.borrow_mut();
            (contents.columns, contents.picture) = (columns, picture);
            contents.tiles.clear();
            contents.arrows.clear();
        }
        let rows = self.contents.borrow().rows();
        // Rows of files drag one by one, and land on a folder's row to move
        // into it; a row of tiles does not, its tiles do. Either way the list
        // takes what is dragged from elsewhere, such as an Obj to make a prefab of.
        ui.set_rows_draggable(self.files, columns == 0);
        ui.accept_row_drops(self.files, columns == 0);
        ui.accept_drops(self.files, true);
        if columns == 0 {
            ui.set_item_height(self.files, ROW);
            ui.bind_list(self.files, rows, FileRows(self.contents.clone()));
        } else {
            ui.set_item_height(self.files, picture + TILE_BELOW);
            ui.bind_list(self.files, rows, TileRows(self.contents.clone()));
        }
    }

    /// The project path of a file or folder being dragged, when it is one of
    /// this view's: a tile, or a row of the open folder's, to move.
    pub fn dragged_entry(&self, source: &DragSource) -> Option<String> {
        let contents = self.contents.borrow();
        let at = match *source {
            DragSource::Row { list, index, .. } if list == self.files && contents.columns == 0 => index,
            DragSource::Element(tile) => contents.tiles.iter().find(|(node, _)| *node == tile)?.1?,
            _ => return None,
        };
        contents.entry_path(*contents.listing.get(at)?)
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
        match *contents.listing.get(at)? {
            Entry::File(file) => {
                let file = &contents.files[file];
                matches!(file.kind, FileKind::Asset(_)).then(|| file.path.clone())
            }
            Entry::Part(file, part) => contents.part(file, part)?.reference.clone(),
            Entry::Folder(_) => None,
        }
    }

    /// Shows a model's or prefab's parts, by its path, as the editor read
    /// them when it was opened out.
    pub fn set_parts(&mut self, ui: &mut Ui, path: &str, parts: Vec<AssetPart>) {
        self.contents.borrow_mut().parts.insert(path.to_string(), parts);
        self.relist(ui);
    }

    /// Lets prefabs open out to their Objs, or closes those opened, as the
    /// setting says.
    pub fn set_prefab_parts(&mut self, ui: &mut Ui, on: bool) {
        let mut contents = self.contents.borrow_mut();
        if contents.prefab_parts == on {
            return;
        }
        contents.prefab_parts = on;
        drop(contents);
        self.relist(ui);
    }

    /// The file whose opening arrow was clicked.
    fn arrow_clicked(&self, ui: &Ui) -> Option<usize> {
        let contents = self.contents.borrow();
        let (_, at) = contents.arrows.iter().find(|(arrow, at)| at.is_some() && ui.clicked(*arrow))?;
        match contents.listing.get((*at)?)? {
            Entry::File(file) => Some(*file),
            _ => None,
        }
    }

    /// Opens a file out to its parts, or closes it. A prefab's are read from
    /// its file; a model's the editor is asked for, the first time.
    fn open_out(&mut self, ui: &mut Ui, file: usize) -> Option<ProjectEvent> {
        let mut contents = self.contents.borrow_mut();
        let (path, kind) = (contents.files[file].path.clone(), contents.files[file].kind);
        if !contents.opened.remove(&path) {
            contents.opened.insert(path.clone());
            if kind == FileKind::Asset(AssetKind::Prefab) {
                let objs = prefab_objs(&self.root.join(&path));
                contents.parts.insert(path.clone(), objs);
            }
        }
        let wants = contents.opened.contains(&path) && !contents.parts.contains_key(&path);
        drop(contents);
        self.relist(ui);
        wants.then_some(ProjectEvent::WantsParts(path))
    }

    /// Lists the open folder again, as after a file was opened out or
    /// closed, keeping the place.
    fn relist(&mut self, ui: &mut Ui) {
        let mut contents = self.contents.borrow_mut();
        contents.listing = contents.list(self.open);
        let rows = contents.rows();
        let selected = contents.selected.and_then(|s| contents.listing.iter().position(|&e| e == s));
        let columns = contents.columns;
        drop(contents);
        ui.set_list_count(self.files, rows);
        if columns == 0 {
            ui.set_selection(self.files, &selected.into_iter().collect::<Vec<_>>());
        }
        ui.refresh_list(self.files);
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

/// A prefab's Objs, by name, as its file lists them.
fn prefab_objs(file: &Path) -> Vec<AssetPart> {
    let Ok(table) = std::fs::read_to_string(file).unwrap_or_default().parse::<toml::Table>() else { return Vec::new() };
    let objs = table.get("obj").and_then(|objs| objs.as_array()).cloned().unwrap_or_default();
    let names = objs.iter().filter_map(|obj| obj.get("name")?.as_str().map(str::to_string));
    names.map(|name| AssetPart { name, icon: "obj", reference: None }).collect()
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
        picture: 0.0,
        selected: None,
        pinged: None,
        tiles: Vec::new(),
        previews: HashMap::new(),
        parts: HashMap::new(),
        opened: HashSet::new(),
        prefab_parts: false,
        arrows: Vec::new(),
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
pub(crate) fn show_in_files(folder: &Path) {
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
