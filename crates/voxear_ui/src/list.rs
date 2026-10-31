//! List and tree views: scrolling lists that keep elements only for the rows
//! on screen, so a list of a hundred thousand items costs what one of twenty
//! does. As in Unity's ListView, each row element is built once by `make`
//! and filled by `bind` whenever it shows a different item, and rows are
//! reused as the list scrolls. Every row has the `list-row` class and
//! matches `:selected` when selected, for sheets to style.
//!
//! A tree view is a list of the items that are open. Each row holds a
//! spacer that indents it by depth (`tree-indent`), a chevron icon that
//! opens and closes it (`tree-arrow`, with `expanded` when open), and then
//! what `make` builds (in `tree-content`). The editor's Hierarchy and
//! Project views are tree views.
//!
//! Every row also ends in a `row-bar`, which the default theme shows as an
//! accent bar down the left of a selected row.
//!
//! Rows can be dragged ([`Ui::set_rows_draggable`], and see `drag`): while
//! one is, its list has `dragging-rows` and the row under the pointer
//! `drop-target`. Letting go on another row, or below the rows, is a drop in
//! the list, which [`Ui::dropped`] reports for the owner to act on, as the
//! Hierarchy reparents Objs.

use std::collections::HashSet;

use taffy::{Dimension, LengthPercentageAuto, Position, Rect};

use crate::{
    NodeId, Style, Ui,
    drag::DragSource,
    selector::States,
    tree::{Element, Kind},
};

/// Builds and fills a list's rows.
pub trait ListItems {
    /// Builds what a row shows, once for each row element.
    fn make(&mut self, ui: &mut Ui, row: NodeId);
    /// Fills a row with item `index`, whenever it shows a different one.
    fn bind(&mut self, ui: &mut Ui, row: NodeId, index: usize);
}

/// A [`ListItems`] from two closures.
pub struct ItemFns<M, B> {
    pub make: M,
    pub bind: B,
}

impl<M: FnMut(&mut Ui, NodeId), B: FnMut(&mut Ui, NodeId, usize)> ListItems for ItemFns<M, B> {
    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        (self.make)(ui, row);
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, index: usize) {
        (self.bind)(ui, row, index);
    }
}

/// The items of a tree, by id, and how to show them.
pub trait TreeItems {
    /// The items at the top (`None`), or in `parent`, in order.
    fn children(&self, parent: Option<u64>) -> Vec<u64>;
    /// Whether an item has anything in it, for its arrow. Override it when
    /// that is cheaper than listing the children.
    fn has_children(&self, item: u64) -> bool {
        !self.children(Some(item)).is_empty()
    }
    fn make(&mut self, ui: &mut Ui, row: NodeId);
    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64);
}

/// The height rows get when nothing says otherwise.
pub(crate) const ITEM_HEIGHT: f32 = 24.0;
/// How far each level of a tree indents.
const INDENT: f32 = 14.0;
/// A row let go of: in which list, which row, and on which row, if any. In
/// a tree, the same as items.
pub(crate) struct RowDrop {
    list: taffy::NodeId,
    index: usize,
    onto: Option<usize>,
    items: Option<(u64, Option<u64>)>,
}

pub(crate) struct ListState {
    pub item_height: f32,
    count: usize,
    /// In the flow, as tall as every row together, so the list scrolls that far.
    spacer: taffy::NodeId,
    rows: Vec<Row>,
    /// Sorted.
    selection: Vec<usize>,
    /// Where a shift-click extends from.
    anchor: Option<usize>,
    pub changed: bool,
    /// Every row shows something else, so all are bound again.
    rebind: bool,
    source: Source,
    draggable: bool,
}

struct Row {
    node: taffy::NodeId,
    /// What `make` and `bind` get: the row, or a tree row's content.
    content: taffy::NodeId,
    /// A tree row's spacer and arrow.
    indent: Option<taffy::NodeId>,
    arrow: Option<taffy::NodeId>,
    index: Option<usize>,
}

enum Source {
    Empty,
    List(Box<dyn ListItems>),
    Tree(TreeState),
}

struct TreeState {
    items: Box<dyn TreeItems>,
    expanded: HashSet<u64>,
    /// Every open item, in order: its id, depth and whether it has children.
    open: Vec<(u64, u32, bool)>,
}

impl TreeState {
    fn flatten(&mut self) {
        self.open.clear();
        let mut stack: Vec<(u64, u32)> = self.items.children(None).into_iter().rev().map(|id| (id, 0)).collect();
        while let Some((id, depth)) = stack.pop() {
            let has_children = self.items.has_children(id);
            self.open.push((id, depth, has_children));
            if has_children && self.expanded.contains(&id) {
                stack.extend(self.items.children(Some(id)).into_iter().rev().map(|child| (child, depth + 1)));
            }
        }
    }
}

impl Ui {
    /// A list of `count` items, `item_height` logical pixels apart.
    pub fn list_view(
        &mut self,
        parent: NodeId,
        item_height: f32,
        count: usize,
        items: impl ListItems + 'static,
    ) -> NodeId {
        let list = self.add(parent, Element::new(Kind::ListView, Style::column().scroll()));
        self.set_item_height(list, item_height);
        self.bind_list(list, count, items);
        list
    }

    /// A tree of `items`, every row `item_height` logical pixels tall,
    /// starting with everything closed.
    pub fn tree_view(&mut self, parent: NodeId, item_height: f32, items: impl TreeItems + 'static) -> NodeId {
        let tree = self.add(parent, Element::new(Kind::TreeView, Style::column().scroll()));
        self.set_item_height(tree, item_height);
        self.bind_tree(tree, items);
        tree
    }

    /// Gives a list, such as one declared in markup, its items.
    pub fn bind_list(&mut self, list: NodeId, count: usize, items: impl ListItems + 'static) {
        let Some(state) = self.list_state(list) else { return };
        state.source = Source::List(Box::new(items));
        state.count = count;
        self.reset_rows(list);
    }

    /// Gives a tree, such as one declared in markup, its items.
    pub fn bind_tree(&mut self, tree: NodeId, items: impl TreeItems + 'static) {
        let Some(state) = self.list_state(tree) else { return };
        let mut tree_state = TreeState { items: Box::new(items), expanded: HashSet::new(), open: Vec::new() };
        tree_state.flatten();
        state.count = tree_state.open.len();
        state.source = Source::Tree(tree_state);
        self.reset_rows(tree);
    }

    /// Rows already made were made for the old items, so they go.
    fn reset_rows(&mut self, list: NodeId) {
        let rows = self.list_state(list).map(|state| std::mem::take(&mut state.rows)).unwrap_or_default();
        for row in rows {
            self.remove(NodeId(row.node));
        }
        if let Some(state) = self.list_state(list) {
            state.selection.clear();
            state.rebind = true;
        }
    }

    pub fn set_item_height(&mut self, list: NodeId, height: f32) {
        if let Some(state) = self.list_state(list) {
            state.item_height = height.max(1.0);
            state.rebind = true;
        }
    }

    /// The list's items changed in number. Every row is filled again.
    pub fn set_list_count(&mut self, list: NodeId, count: usize) {
        let Some(state) = self.list_state(list) else { return };
        state.count = count;
        state.rebind = true;
        let before = state.selection.len();
        state.selection.retain(|&i| i < count);
        state.changed |= state.selection.len() != before;
    }

    /// The items changed: every row is filled again, and a tree reads which
    /// items it has again, keeping what was open and selected.
    pub fn refresh_list(&mut self, list: NodeId) {
        let Some(state) = self.list_state(list) else { return };
        state.rebind = true;
        let Source::Tree(tree) = &mut state.source else { return };
        let selected: Vec<u64> = state.selection.iter().filter_map(|&i| tree.open.get(i).map(|o| o.0)).collect();
        tree.flatten();
        state.count = tree.open.len();
        state.selection =
            tree.open.iter().enumerate().filter(|(_, o)| selected.contains(&o.0)).map(|(i, _)| i).collect();
    }

    /// Opens or closes a tree item.
    pub fn set_expanded(&mut self, tree: NodeId, item: u64, expanded: bool) {
        let Some(state) = self.list_state(tree) else { return };
        let Source::Tree(tree_state) = &mut state.source else { return };
        let changed = if expanded { tree_state.expanded.insert(item) } else { tree_state.expanded.remove(&item) };
        if changed {
            self.refresh_list(tree);
        }
    }

    pub fn is_expanded(&self, tree: NodeId, item: u64) -> bool {
        let state = self.element(tree).and_then(|e| e.list.as_ref());
        state.is_some_and(|s| matches!(&s.source, Source::Tree(t) if t.expanded.contains(&item)))
    }

    /// The selected rows, in order.
    pub fn selection(&self, list: NodeId) -> &[usize] {
        self.element(list).and_then(|e| e.list.as_ref()).map_or(&[], |s| &s.selection)
    }

    /// The selected items of a tree, in order.
    pub fn selected_items(&self, tree: NodeId) -> Vec<u64> {
        let Some(state) = self.element(tree).and_then(|e| e.list.as_ref()) else { return Vec::new() };
        let Source::Tree(tree) = &state.source else { return Vec::new() };
        state.selection.iter().filter_map(|&i| tree.open.get(i).map(|o| o.0)).collect()
    }

    /// Selects one item of a tree, if it is among the open rows, and scrolls to it.
    pub fn select_item(&mut self, tree: NodeId, item: u64) -> bool {
        let Some(state) = self.list_state(tree) else { return false };
        let Source::Tree(tree_state) = &state.source else { return false };
        let Some(index) = tree_state.open.iter().position(|open| open.0 == item) else { return false };
        self.set_selection(tree, &[index]);
        self.scroll_to_index(tree, index);
        true
    }

    pub fn set_selection(&mut self, list: NodeId, indices: &[usize]) {
        let Some(state) = self.list_state(list) else { return };
        let mut selection: Vec<usize> = indices.iter().copied().filter(|&i| i < state.count).collect();
        selection.sort_unstable();
        selection.dedup();
        if selection != state.selection {
            state.selection = selection;
            state.anchor = state.selection.first().copied();
            state.changed = true;
            self.show_selection(list);
        }
    }

    /// Whether the selection changed since the last frame.
    pub fn selection_changed(&self, list: NodeId) -> bool {
        self.element(list).and_then(|e| e.list.as_ref()).is_some_and(|s| s.changed)
    }

    /// Scrolls the least distance that shows row `index` whole.
    pub fn scroll_to_index(&mut self, list: NodeId, index: usize) {
        let Some(height) = self.element(list).and_then(|e| e.list.as_ref()).map(|s| s.item_height) else { return };
        let [_, _, _, viewport] = self.viewport(list);
        let [x, y] = self.scroll_offset(list);
        let (top, bottom) = (index as f32 * height, (index + 1) as f32 * height);
        let y = if top < y {
            top
        } else if bottom > y + viewport {
            bottom - viewport
        } else {
            y
        };
        // The limits may predate rows added since, so the offset is kept as asked until the next layout.
        if let Some(element) = self.element_mut(list) {
            element.scroll = [x, y.max(0.0)];
        }
    }

    /// Lets the list's rows be dragged onto one another, or below the last.
    pub fn set_rows_draggable(&mut self, list: NodeId, draggable: bool) {
        if let Some(state) = self.list_state(list) {
            state.draggable = draggable;
        }
    }

    /// A row dragged and let go of in `list` since the last frame: its
    /// index, and the index of the row it landed on, or `None` below the rows.
    pub fn dropped(&self, list: NodeId) -> Option<(usize, Option<usize>)> {
        self.row_drops.iter().find(|d| d.list == list.0).map(|d| (d.index, d.onto))
    }

    /// The item of the tree row under the pointer, if any: for a context
    /// click, whether it was on a row or the space below them.
    pub fn item_at_pointer(&self, tree: NodeId) -> Option<u64> {
        let target = *self.hit_path().last()?;
        let (tree, row) = self.row_at(target).filter(|(found, _)| *found == tree)?;
        let state = self.element(tree)?.list.as_ref()?;
        let index = state.rows.iter().find(|r| r.node == row)?.index?;
        match &state.source {
            Source::Tree(items) => items.open.get(index).map(|o| o.0),
            _ => None,
        }
    }

    /// [`dropped`](Self::dropped) for a tree, by item.
    pub fn dropped_item(&self, tree: NodeId) -> Option<(u64, Option<u64>)> {
        self.row_drops.iter().find(|d| d.list == tree.0).and_then(|d| d.items)
    }

    fn list_state(&mut self, list: NodeId) -> Option<&mut ListState> {
        let kind = self.kind(list)?;
        if !matches!(kind, Kind::ListView | Kind::TreeView) {
            return None;
        }
        if self.element(list)?.list.is_none() {
            let spacer = Style { pickable: false, ..Style::default() };
            let spacer = self.add(list, Element::new(Kind::Panel, spacer)).0;
            let state = ListState {
                item_height: ITEM_HEIGHT,
                count: 0,
                spacer,
                rows: Vec::new(),
                selection: Vec::new(),
                anchor: None,
                changed: false,
                rebind: true,
                source: Source::Empty,
                draggable: false,
            };
            self.element_mut(list)?.list = Some(Box::new(state));
            self.lists.push(list.0);
        }
        self.element_mut(list)?.list.as_deref_mut()
    }

    /// Makes and binds rows for what is on screen in every list. Returns
    /// whether any changed, which then need laying out.
    pub(crate) fn update_lists(&mut self) -> bool {
        let mut changed = false;
        for list in self.lists.clone() {
            let list = NodeId(list);
            let Some(mut state) = self.element_mut(list).and_then(|e| e.list.take()) else { continue };
            changed |= state.update(self, list);
            if let Some(element) = self.element_mut(list) {
                element.list = Some(state);
            }
        }
        changed
    }

    pub(crate) fn clear_selection_changes(&mut self) {
        for &list in &self.lists {
            if let Some(state) = self.tree.get_node_context_mut(list).and_then(|e| e.list.as_mut()) {
                state.changed = false;
            }
        }
    }

    /// A click inside a list's row: on a tree's arrow it opens or closes the
    /// item, and anywhere else it selects, with Ctrl adding and Shift
    /// extending, as in a file manager.
    pub(crate) fn row_clicked(&mut self, target: taffy::NodeId) {
        let Some((list, row)) = self.row_at(target) else { return };
        let modifiers = self.modifiers;
        let Some(state) = self.list_state(list) else { return };
        let Some(found) = state.rows.iter().find(|r| r.node == row) else { return };
        let (Some(index), arrow) = (found.index, found.arrow) else { return };
        let tree_item = match &state.source {
            Source::Tree(tree) => tree.open.get(index).copied(),
            _ => None,
        };
        let (mut selection, anchor) = (state.selection.clone(), state.anchor);
        if let (Some(arrow), Some((item, _, has_children))) = (arrow, tree_item)
            && self.is_within(NodeId(target), NodeId(arrow))
        {
            if has_children {
                let open = !self.is_expanded(list, item);
                self.set_expanded(list, item, open);
            }
            return;
        }
        match (modifiers.shift, modifiers.control, anchor) {
            (true, _, Some(anchor)) => selection = (anchor.min(index)..=anchor.max(index)).collect(),
            (_, true, _) if selection.contains(&index) => selection.retain(|&i| i != index),
            (_, true, _) => selection.push(index),
            _ => selection = vec![index],
        }
        self.set_selection(list, &selection);
        if let Some(state) = self.list_state(list) {
            state.anchor = if modifiers.shift { anchor } else { Some(index) };
        }
    }

    /// A context click inside a list's row selects it, unless it is selected
    /// already, so a menu acts on what was clicked.
    pub(crate) fn row_context_clicked(&mut self, target: taffy::NodeId) {
        let Some((list, index)) = self.row_index_at(target) else { return };
        let selected = self.list_state(list).is_some_and(|state| state.selection.contains(&index));
        if !selected {
            self.set_selection(list, &[index]);
        }
    }

    /// The row `target` is in, when its list's rows drag: the list, the
    /// row's index and, in a tree, its item.
    pub(crate) fn draggable_row(&mut self, target: taffy::NodeId) -> Option<DragSource> {
        let (list, index) = self.row_index_at(target)?;
        let state = self.list_state(list).filter(|state| state.draggable)?;
        let item = match &state.source {
            Source::Tree(tree) => tree.open.get(index).map(|o| o.0),
            _ => None,
        };
        Some(DragSource::Row { list, index, item })
    }

    /// The row element showing item `index`, if it is on screen.
    pub(crate) fn row_node(&self, list: NodeId, index: usize) -> Option<taffy::NodeId> {
        let state = self.element(list)?.list.as_ref()?;
        state.rows.iter().find(|r| r.index == Some(index)).map(|r| r.node)
    }

    /// Where a row dragged in its own list would land: another row under the
    /// pointer, and its index.
    pub(crate) fn landing_row(&mut self, list: NodeId, index: usize) -> Option<taffy::NodeId> {
        self.drop_row(list).filter(|&(_, i)| i != index).map(|(row, _)| row)
    }

    /// A row let go of inside its own list: a drop on the row under the
    /// pointer, or below the rows.
    pub(crate) fn drop_in_list(&mut self, list: NodeId, index: usize) {
        let onto = self.drop_row(list).map(|(_, at)| at);
        if onto == Some(index) {
            return;
        }
        let items = match self.element(list).and_then(|e| e.list.as_ref()).map(|state| &state.source) {
            Some(Source::Tree(tree)) => {
                let item = |at: usize| tree.open.get(at).map(|o| o.0);
                item(index).map(|from| (from, onto.and_then(item)))
            }
            _ => None,
        };
        self.row_drops.push(RowDrop { list: list.0, index, onto, items });
    }

    /// The row of `list` under the pointer, and its index.
    fn drop_row(&mut self, list: NodeId) -> Option<(taffy::NodeId, usize)> {
        let target = *self.hit_path().last()?;
        let (list, row) = self.row_at(target).filter(|(found, _)| *found == list)?;
        let index = self.list_state(list)?.rows.iter().find(|r| r.node == row)?.index?;
        Some((row, index))
    }

    /// The list and the index of the row that `node` is in, if any.
    fn row_index_at(&mut self, node: taffy::NodeId) -> Option<(NodeId, usize)> {
        let (list, row) = self.row_at(node)?;
        let index = self.list_state(list)?.rows.iter().find(|r| r.node == row)?.index?;
        Some((list, index))
    }

    /// The list and row that `node` is in, if any.
    fn row_at(&self, node: taffy::NodeId) -> Option<(NodeId, taffy::NodeId)> {
        let mut at = Some(node);
        while let Some(current) = at {
            if let Some(list) = self.tree.get_node_context(current).and_then(|e| e.row_of) {
                return Some((NodeId(list), current));
            }
            at = self.tree.parent(current);
        }
        None
    }

    /// Marks the rows on screen that are selected.
    fn show_selection(&mut self, list: NodeId) {
        let Some(state) = self.element(list).and_then(|e| e.list.as_ref()) else { return };
        let rows: Vec<(taffy::NodeId, bool)> = state
            .rows
            .iter()
            .map(|r| (r.node, r.index.is_some_and(|i| state.selection.binary_search(&i).is_ok())))
            .collect();
        for (row, selected) in rows {
            self.set_state(NodeId(row), States::SELECTED, selected);
        }
    }
}

impl ListState {
    /// Makes sure rows show exactly the items on screen. Returns whether
    /// anything changed.
    fn update(&mut self, ui: &mut Ui, list: NodeId) -> bool {
        let mut changed = false;
        let height = self.item_height;
        let total = Dimension::length(self.count as f32 * height);
        let spacer = NodeId(self.spacer);
        if ui.element(spacer).is_some_and(|e| e.base.layout.size.height != total) {
            let mut style = ui.element(spacer).map(|e| e.base.clone()).unwrap_or_default();
            style.layout.size.height = total;
            ui.set_style(spacer, style);
            changed = true;
        }

        let [_, _, _, viewport] = ui.viewport(list);
        let scroll = ui.scroll_offset(list)[1];
        let end = (((scroll + viewport) / height).ceil() as usize + 1).min(self.count);
        let first = ((scroll / height).floor() as usize).min(end);
        let rebind = std::mem::take(&mut self.rebind);
        for row in &mut self.rows {
            if rebind || row.index.is_some_and(|i| !(first..end).contains(&i)) {
                row.index = None;
            }
        }
        for index in first..end {
            if self.rows.iter().any(|r| r.index == Some(index)) {
                continue;
            }
            let slot = match self.rows.iter().position(|r| r.index.is_none()) {
                Some(slot) => slot,
                None => {
                    let row = self.make_row(ui, list);
                    self.rows.push(row);
                    self.rows.len() - 1
                }
            };
            self.rows[slot].index = Some(index);
            self.bind_row(ui, slot, index);
            changed = true;
        }
        for row in &self.rows {
            if row.index.is_none() && ui.is_visible(NodeId(row.node)) {
                ui.set_visible(NodeId(row.node), false);
                changed = true;
            }
        }
        changed
    }

    fn make_row(&mut self, ui: &mut Ui, list: NodeId) -> Row {
        let style =
            Style { layout: taffy::Style { position: Position::Absolute, ..Style::row().layout }, ..Style::default() };
        let row = ui.add(list, Element { row_of: Some(list.0), ..Element::new(Kind::Panel, style) });
        ui.add_class(row, "list-row");
        let (content, indent, arrow) = match &self.source {
            Source::Tree(_) => {
                ui.add_class(row, "tree-row");
                // Its own element, so a sheet's padding on rows cannot undo the indent.
                let indent = ui.panel(row, Style::default());
                ui.add_class(indent, "tree-indent");
                let mut arrow_style = Style::default();
                arrow_style.layout.size.width = Dimension::length(INDENT);
                arrow_style.layout.flex_shrink = 0.0;
                let arrow = ui.pressable_icon(row, "", arrow_style);
                ui.add_class(arrow, "tree-arrow");
                let content = ui.panel(row, Style::row().grow());
                ui.add_class(content, "tree-content");
                (content, Some(indent.0), Some(arrow.0))
            }
            _ => (row, None, None),
        };
        match &mut self.source {
            Source::List(items) => items.make(ui, content),
            Source::Tree(tree) => tree.items.make(ui, content),
            Source::Empty => {}
        }
        let mut bar = Style { pickable: false, ..Style::default() };
        bar.layout.position = Position::Absolute;
        let bar = ui.panel(row, bar);
        ui.add_class(bar, "row-bar");
        Row { node: row.0, content: content.0, indent, arrow, index: None }
    }

    fn bind_row(&mut self, ui: &mut Ui, slot: usize, index: usize) {
        let Row { node, content, indent, arrow, .. } = self.rows[slot];
        let (row, content) = (NodeId(node), NodeId(content));
        let mut style = ui.element(row).map(|e| e.base.clone()).unwrap_or_default();
        let layout = &mut style.layout;
        layout.display = taffy::Display::Flex;
        layout.inset = Rect {
            left: LengthPercentageAuto::length(0.0),
            right: LengthPercentageAuto::length(0.0),
            top: LengthPercentageAuto::length(index as f32 * self.item_height),
            bottom: LengthPercentageAuto::auto(),
        };
        layout.size.height = Dimension::length(self.item_height);
        ui.set_style(row, style);
        if let (Source::Tree(tree), Some(indent)) = (&self.source, indent) {
            let mut style = Style::default();
            style.layout.size.width = Dimension::length(tree.open[index].1 as f32 * INDENT);
            style.layout.flex_shrink = 0.0;
            ui.set_style(NodeId(indent), style);
        }
        ui.set_state(row, States::SELECTED, self.selection.binary_search(&index).is_ok());
        match &mut self.source {
            Source::List(items) => items.bind(ui, content, index),
            Source::Tree(tree) => {
                let (item, _, has_children) = tree.open[index];
                let expanded = tree.expanded.contains(&item);
                if let Some(arrow) = arrow {
                    let icon = match (has_children, expanded) {
                        (false, _) => "",
                        (true, true) => "chevron-down",
                        (true, false) => "chevron-right",
                    };
                    ui.set_icon(NodeId(arrow), icon);
                    ui.set_class(NodeId(arrow), "expanded", expanded);
                }
                tree.items.bind(ui, content, item);
            }
            Source::Empty => {}
        }
    }
}
