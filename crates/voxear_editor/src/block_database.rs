//! The Blocks panel: every block of the project, each by its picture and
//! its name, as a grid that makes only the rows on screen. Typing in its
//! search box narrows it, the best match first.
//!
//! A click picks a block, which the Inspector then shows, to change its
//! look and its rules. A double click has the Scene view's brush build
//! with it. A block dragged from here goes wherever a block is taken, as
//! onto a Block World's Block. **New Block** makes one in the project's
//! Blocks folder, **Starter Blocks** makes a set of slabs, stairs, fences,
//! doors and more to start from (see `starter_blocks`), **Duplicate**
//! copies the one picked, and **Delete** moves
//! it to the project's trash: where worlds hold it, only once that is said
//! and asked again, as its blocks in them are then missing.
//!
//! The panel asks; the editor does what it asks (see [`BlocksEvent`]).

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use voxear_assets::{AssetDb, AssetKind};
use voxear_ui::{DragSource, ImageRef, ListItems, MenuItem, NodeId, Style, Ui};

use crate::palette::score;

/// How big a block's picture is, what its tile takes across beside it with
/// the gap after it, and down beside it for its name, in logical pixels.
const PICTURE: f32 = 56.0;
const TILE_SIDES: f32 = 28.0;
const TILE_BELOW: f32 = 34.0;

/// What the panel asks of the editor, each block by its path.
#[derive(Clone, Debug, PartialEq)]
pub enum BlocksEvent {
    /// A block was picked: the Inspector shows it.
    Selected(String),
    /// A block was double clicked: the brush builds with it.
    BuildWith(String),
    New,
    /// Starter Blocks: the starter blocks made.
    Starters,
    Duplicate(String),
    /// Delete was asked for; and asked again, for a block worlds hold.
    Delete(String),
    DeleteHeld(String),
}

/// A block of the project: its file's path, and its name.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    path: String,
    name: String,
}

/// What the grid shows, shared with its rows.
#[derive(Default)]
struct Shown {
    every: Vec<Entry>,
    /// The blocks the search leaves, by their place in `every`.
    listing: Vec<usize>,
    columns: usize,
    /// Each tile made, with the place in `listing` it shows.
    tiles: Vec<(NodeId, Option<usize>)>,
    /// Pictures drawn so far, by path.
    previews: HashMap<String, ImageRef>,
    selected: Option<String>,
}

impl Shown {
    fn entry(&self, at: usize) -> Option<&Entry> {
        self.every.get(*self.listing.get(at)?)
    }

    fn rows(&self) -> usize {
        self.listing.len().div_ceil(self.columns.max(1))
    }
}

/// The grid's rows, each a row of tiles: a block's picture over its name.
struct TileRows(Rc<RefCell<Shown>>);

impl ListItems for TileRows {
    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        ui.add_class(row, "tile-row");
        let columns = self.0.borrow().columns;
        for _ in 0..columns {
            let tile = ui.panel(row, Style::column());
            ui.add_class(tile, "tile");
            ui.set_draggable(tile, true);
            ui.set_inline_style(tile, &format!("width: {}px", PICTURE + TILE_SIDES - 2.0));
            let thumb = ui.panel(tile, Style::default());
            ui.add_class(thumb, "tile-thumb");
            ui.set_inline_style(thumb, &format!("width: {PICTURE}px; height: {PICTURE}px"));
            let icon = ui.icon(thumb, "block");
            ui.add_class(icon, "tile-icon");
            let image = ui.image(thumb);
            ui.add_class(image, "thumb-image");
            ui.set_visible(image, false);
            let name = ui.add_label(tile, "");
            ui.add_class(name, "tile-name");
            self.0.borrow_mut().tiles.push((tile, None));
        }
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, index: usize) {
        let mut shown = self.0.borrow_mut();
        let tiles: Vec<NodeId> = ui.children(row).into_iter().filter(|&n| ui.has_class(n, "tile")).collect();
        for (column, tile) in tiles.into_iter().enumerate() {
            let at = index * shown.columns + column;
            let entry = shown.entry(at).cloned();
            if let Some(slot) = shown.tiles.iter_mut().find(|(node, _)| *node == tile) {
                slot.1 = entry.as_ref().map(|_| at);
            }
            ui.set_visible(tile, entry.is_some());
            let Some(entry) = entry else { continue };
            let parts = ui.children(tile);
            let pictures = ui.children(parts[0]);
            let preview = shown.previews.get(&entry.path).copied();
            ui.set_visible(pictures[0], preview.is_none());
            ui.set_image(pictures[1], preview);
            ui.set_visible(pictures[1], preview.is_some());
            ui.set_text(parts[1], &entry.name);
            ui.set_tooltip(tile, &entry.path, None);
            ui.set_checked(tile, shown.selected.as_deref() == Some(entry.path.as_str()));
        }
    }
}

pub struct BlockDatabase {
    search: NodeId,
    count: NodeId,
    new: NodeId,
    starters: NodeId,
    duplicate: NodeId,
    delete: NodeId,
    list: NodeId,
    empty: NodeId,
    shown: Rc<RefCell<Shown>>,
    /// What was typed when the grid was last narrowed.
    typed: String,
    /// The block Delete was asked for while worlds hold it, until it is
    /// asked again or let be.
    deleting: Option<String>,
}

impl BlockDatabase {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "console-toolbar");
        let search = ui.panel(bar, Style::row());
        ui.add_class(search, "search");
        ui.add_class(search, "blocks-search");
        let icon = ui.icon(search, "search");
        ui.add_class(icon, "search-icon");
        let field = ui.text_field(search, "");
        ui.set_placeholder(field, "Find a block");
        ui.add_class(field, "search-field");
        let count = ui.add_label(bar, "");
        ui.add_class(count, "entities-summary");
        let button = |ui: &mut Ui, text: &str| {
            let button = ui.button(bar, text);
            ui.add_class(button, "console-button");
            button
        };
        let new = button(ui, "New Block");
        let starters = button(ui, "Starter Blocks");
        ui.set_tooltip(starters, "Makes slabs, stairs, fences, doors, levers and more in the Blocks folder", None);
        let duplicate = button(ui, "Duplicate");
        let delete = button(ui, "Delete");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");
        let shown: Rc<RefCell<Shown>> = Rc::default();
        let list = ui.list_view(root, PICTURE + TILE_BELOW, 0, TileRows(shown.clone()));
        ui.add_class(list, "fill");
        ui.add_class(list, "files");
        let empty = ui.add_label(root, "No blocks");
        ui.add_class(empty, "entities-hint");
        ui.set_visible(empty, false);
        let typed = String::new();
        Self { search: field, count, new, starters, duplicate, delete, list, empty, shown, typed, deleting: None }
    }

    /// Reads the project's blocks again where they are others than it
    /// shows, lays the grid out for the room there is, and gives what was
    /// asked for. Nothing while the panel is not `showing`.
    pub fn update(&mut self, ui: &mut Ui, db: &AssetDb, showing: bool) -> Vec<BlocksEvent> {
        if !showing {
            return Vec::new();
        }
        let mut every: Vec<Entry> =
            db.entries().filter(|entry| entry.kind == AssetKind::Block).map(|entry| entry_of(&entry.path)).collect();
        every.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.path.cmp(&b.path)));
        let typed = ui.text(self.search);
        let changed = every != self.shown.borrow().every;
        if changed || typed != self.typed {
            self.typed = typed;
            let mut shown = self.shown.borrow_mut();
            shown.every = every;
            shown.listing = narrowed(&shown.every, &self.typed);
            let gone = shown.selected.as_ref().is_some_and(|path| !shown.every.iter().any(|e| e.path == *path));
            if gone {
                shown.selected = None;
            }
            drop(shown);
            self.lay_out(ui, true);
        } else {
            self.lay_out(ui, false);
        }
        let selected = self.shown.borrow().selected.clone();
        ui.set_enabled(self.duplicate, selected.is_some());
        ui.set_enabled(self.delete, selected.is_some());

        let mut events = Vec::new();
        if ui.clicked(self.new) {
            events.push(BlocksEvent::New);
        }
        if ui.clicked(self.starters) {
            events.push(BlocksEvent::Starters);
        }
        if let Some(path) = selected.clone().filter(|_| ui.clicked(self.duplicate)) {
            events.push(BlocksEvent::Duplicate(path));
        }
        if let Some(path) = selected.filter(|_| ui.clicked(self.delete)) {
            events.push(BlocksEvent::Delete(path));
        }
        // The menu a held block's Delete opened: its second item deletes it all the same.
        if let Some(choice) = ui.menu_choice(self.delete)
            && let Some(path) = self.deleting.take().filter(|_| choice == 1)
        {
            events.push(BlocksEvent::DeleteHeld(path));
        }
        let clicked = {
            let shown = self.shown.borrow();
            let tile = shown.tiles.iter().find(|(tile, at)| at.is_some() && ui.clicked(*tile));
            tile.and_then(|(tile, at)| Some((shown.entry((*at)?)?.path.clone(), ui.double_clicked(*tile))))
        };
        if let Some((path, twice)) = clicked {
            if self.shown.borrow().selected.as_deref() != Some(path.as_str()) {
                self.select(ui, Some(&path));
                events.push(BlocksEvent::Selected(path.clone()));
            }
            if twice {
                events.push(BlocksEvent::BuildWith(path));
            }
        }
        events
    }

    /// Fits as many tiles across as there is room for, and fills the rows
    /// again where the tiles across or, `changed`, the blocks are others.
    fn lay_out(&mut self, ui: &mut Ui, changed: bool) {
        let width = ui.rect(self.list)[2];
        let columns = (((width - 12.0) / (PICTURE + TILE_SIDES)).floor().max(1.0)) as usize;
        let refit = columns != self.shown.borrow().columns;
        if !refit && !changed {
            return;
        }
        let (rows, found, every) = {
            let mut shown = self.shown.borrow_mut();
            if refit {
                shown.columns = columns;
                shown.tiles.clear();
            }
            (shown.rows(), shown.listing.len(), shown.every.len())
        };
        if refit {
            ui.bind_list(self.list, rows, TileRows(self.shown.clone()));
        } else {
            ui.set_list_count(self.list, rows);
            ui.refresh_list(self.list);
        }
        let count = match (found == every, every) {
            (true, 1) => "1 block".to_string(),
            (true, _) => format!("{every} blocks"),
            (false, _) => format!("{found} of {every}"),
        };
        ui.set_text(self.count, &count);
        ui.set_text(self.empty, if every == 0 { "No blocks" } else { "No block by that name" });
        ui.set_visible(self.empty, found == 0);
    }

    /// Picks the block at `path`, or none.
    pub fn select(&mut self, ui: &mut Ui, path: Option<&str>) {
        self.shown.borrow_mut().selected = path.map(str::to_string);
        ui.refresh_list(self.list);
    }

    /// The block picked, by its path.
    pub fn selected(&self) -> Option<String> {
        self.shown.borrow().selected.clone()
    }

    /// Says under Delete that the worlds `holding` hold the block at
    /// `path`, and asks again whether it is to go.
    pub fn ask_delete(&mut self, ui: &mut Ui, path: &str, holding: &[String]) {
        let [x, y, _, height] = ui.rect(self.delete);
        let names: Vec<&str> = holding.iter().map(|world| file_stem(world)).collect();
        let held = match names.as_slice() {
            [one] => format!("In {one}"),
            [one, two] => format!("In {one} and {two}"),
            [one, rest @ ..] => format!("In {one} and {} more worlds", rest.len()),
            [] => String::new(),
        };
        let items = [MenuItem::new(&held).enabled(false), MenuItem::new("Delete Anyway")];
        ui.open_menu(self.delete, (x, y + height + 4.0), &items);
        self.deleting = Some(path.to_string());
    }

    /// The blocks on screen, by path: those to draw pictures of.
    pub fn previewable(&self, ui: &Ui) -> Vec<String> {
        let shown = self.shown.borrow();
        let on_screen = shown.tiles.iter().filter(|(tile, _)| ui.is_visible(*tile));
        on_screen.filter_map(|(_, at)| Some(shown.entry((*at)?)?.path.clone())).collect()
    }

    /// How big a block's picture shows, in logical pixels.
    pub fn preview_size(&self) -> f32 {
        PICTURE
    }

    /// Shows the pictures drawn so far, by path, in place of icons.
    pub fn set_previews(&mut self, ui: &mut Ui, previews: HashMap<String, ImageRef>) {
        if self.shown.borrow().previews == previews {
            return;
        }
        self.shown.borrow_mut().previews = previews;
        ui.refresh_list(self.list);
    }

    /// The block being dragged, by its path, where it is one of this
    /// panel's tiles.
    pub fn dragged(&self, source: &DragSource) -> Option<String> {
        let DragSource::Element(tile) = *source else { return None };
        let shown = self.shown.borrow();
        let at = shown.tiles.iter().find(|(node, _)| *node == tile)?.1?;
        Some(shown.entry(at)?.path.clone())
    }

    /// The blocks the grid shows, by name, in the order it shows them.
    pub fn names(&self) -> Vec<String> {
        let shown = self.shown.borrow();
        shown.listing.iter().map(|&at| shown.every[at].name.clone()).collect()
    }

    /// The tile showing the block at `path`, where it is on screen.
    pub fn tile(&self, path: &str) -> Option<NodeId> {
        let shown = self.shown.borrow();
        let showing = |at: &Option<usize>| at.and_then(|at| shown.entry(at)).is_some_and(|entry| entry.path == path);
        shown.tiles.iter().find(|(_, at)| showing(at)).map(|(tile, _)| *tile)
    }
}

fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn entry_of(path: &str) -> Entry {
    Entry { path: path.to_string(), name: file_stem(path).to_string() }
}

/// The blocks `typed` leaves, by their place in `every`: all of them, in
/// order, with nothing typed, or those whose name or folder it matches,
/// the best first.
fn narrowed(every: &[Entry], typed: &str) -> Vec<usize> {
    let typed = typed.trim();
    if typed.is_empty() {
        return (0..every.len()).collect();
    }
    let mut ranked: Vec<(i32, usize)> = every
        .iter()
        .enumerate()
        .filter_map(|(at, entry)| {
            let by_name = score(typed, &entry.name);
            let by_place = score(typed, &entry.path.replace('/', " ")).map(|s| s - 64 * 20);
            Some((by_name.max(by_place)?, at))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().map(|(_, at)| at).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_narrows_the_blocks_to_those_it_matches_the_best_first() {
        let every: Vec<Entry> =
            ["Blocks/Stone.block", "Blocks/Sandstone.block", "Blocks/Grass.block", "Ores/Gold.block"]
                .map(entry_of)
                .to_vec();
        assert_eq!(narrowed(&every, ""), vec![0, 1, 2, 3], "every block with nothing typed");
        assert_eq!(narrowed(&every, "  "), vec![0, 1, 2, 3]);
        assert_eq!(narrowed(&every, "stone"), vec![0, 1], "Stone before Sandstone");
        assert_eq!(narrowed(&every, "gr"), vec![2]);
        assert_eq!(narrowed(&every, "ores"), vec![3], "a folder's name finds what is in it");
        assert!(narrowed(&every, "zzz").is_empty());
        assert_eq!(every[3].name, "Gold");
    }
}
