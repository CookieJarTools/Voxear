//! The collection field: how the editor shows every list, as Unity 6 shows
//! its reorderable lists. A box holds one item per element, each on a
//! ground of its own with a line between, a grip at its left that drags it
//! to a new place while the others slide out of its way, and a button at its
//! right that takes it out. Under the box, a button adds one. An empty list
//! says so in the box.
//!
//! A script's lists and arrays, a level's scenes and the Build window's
//! lists are all collection fields, so they look and move alike.

use voxear_ui::{NodeId, Style, Ui};

/// A collection field: the box its items go in, what it says when there are
/// none, and its add button.
pub(crate) struct Collection {
    pub frame: NodeId,
    pub items: NodeId,
    pub empty: NodeId,
    pub add: Option<NodeId>,
}

/// One element's item, where its own values go, and its remove button.
pub(crate) struct CollectionItem {
    pub node: NodeId,
    pub body: NodeId,
    pub remove: Option<NodeId>,
}

/// A collection field under `parent`, with `count` items to come. When
/// `editable`, its items drag into a new order and it has an add button,
/// with `add` as its text, or a plus alone.
pub(crate) fn collection(ui: &mut Ui, parent: NodeId, count: usize, editable: bool, add: Option<&str>) -> Collection {
    let frame = ui.panel(parent, Style::column());
    ui.add_class(frame, "collection");
    let items = ui.panel(frame, Style::column());
    ui.add_class(items, "collection-items");
    ui.set_reorderable(items, editable);
    let empty = ui.add_label(frame, "Empty");
    ui.add_class(empty, "collection-empty");
    show_count(ui, items, empty, count);
    let add = editable.then(|| {
        let footer = ui.panel(frame, Style::row());
        ui.add_class(footer, "collection-footer");
        let button = match add {
            Some(text) => ui.button(footer, text),
            None => ui.icon_button(footer, "plus"),
        };
        ui.add_class(button, "collection-add");
        button
    });
    Collection { frame, items, empty, add }
}

/// Shows a collection field as holding `count` items: its box when it has
/// some, and that it is empty when it has none.
pub(crate) fn show_count(ui: &mut Ui, items: NodeId, empty: NodeId, count: usize) {
    ui.set_class(items, "none", count == 0);
    ui.set_visible(empty, count == 0);
}

/// An item at `place` in a collection field's box of `items`, from the top:
/// a grip, its body, and when `editable`, a remove button.
pub(crate) fn item(ui: &mut Ui, items: NodeId, place: usize, editable: bool) -> CollectionItem {
    let row = ui.panel(items, Style::row());
    ui.add_class(row, "collection-item");
    ui.set_class(row, "first", place == 0);
    ui.set_class(row, "odd", place % 2 == 1);
    let grip = ui.icon(row, "grip");
    ui.add_class(grip, "collection-grip");
    ui.set_class(grip, "disabled", !editable);
    if editable {
        ui.set_reorder_handle(grip, true);
    }
    let body = ui.panel(row, Style::column());
    ui.add_class(body, "collection-body");
    let remove = editable.then(|| {
        let button = ui.icon_button(row, "close");
        ui.add_class(button, "collection-remove");
        button
    });
    CollectionItem { node: row, body, remove }
}

/// The move let go of in a collection since the last frame: the item's
/// place before, and after.
pub(crate) fn moved(ui: &Ui, collection: NodeId) -> Option<(usize, usize)> {
    ui.reordered(collection)
}
