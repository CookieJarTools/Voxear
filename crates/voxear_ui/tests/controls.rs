//! Scroll, list and tree views, without a GPU.

use std::{cell::RefCell, rc::Rc};

use voxear_ui::{DropPlace, ItemFns, Modifiers, NodeId, Quad, Style, TextStyle, TreeItems, Ui};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

/// Rows a list has made and is showing, top to bottom.
fn shown_rows(ui: &Ui, list: NodeId) -> Vec<NodeId> {
    let mut rows: Vec<NodeId> =
        ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r)).collect();
    rows.sort_by(|a, b| ui.rect(*a)[1].total_cmp(&ui.rect(*b)[1]));
    rows
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

#[test]
fn scroll_views_clip_scroll_and_drag() {
    let mut ui = Ui::new();
    let root = ui.root();
    let view = ui.panel(root, Style::column().scroll().at(0.0, 0.0).width(200.0).height(200.0));
    let items: Vec<NodeId> = (0..20)
        .map(|i| {
            let item = ui.panel(view, Style::default().height(30.0).background(voxear_ui::Color::WHITE));
            ui.label(item, &format!("Item {i}"), TextStyle::default());
            item
        })
        .collect();
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    let shapes: Vec<&Quad> = frame.quads.iter().filter(|q| q.kind == Quad::SHAPE).collect();
    assert_eq!(shapes.len(), 7 + 1, "the 7 items in sight, and the scrollbar; the rest are culled");
    assert!(shapes.iter().all(|q| q.clip == [0.0, 0.0, 200.0, 200.0]), "clipped to the view");

    ui.pointer_move(100.0, 100.0);
    assert!(ui.pointer_scroll(0.0, 100.0));
    assert_eq!(ui.scroll_offset(view), [0.0, 100.0]);
    assert_eq!(ui.rect(items[5])[1], 50.0, "item 5 moved up by the scroll");
    ui.pointer_scroll(0.0, 10_000.0);
    assert_eq!(ui.scroll_offset(view)[1], 400.0, "no further than the end: 600 of items in 200 of view");
    assert!(!ui.pointer_scroll(0.0, 50.0), "nothing left to scroll, so the wheel is not taken");

    // A press on the track jumps there; dragging the thumb follows it.
    ui.pointer_move(197.0, 5.0);
    assert!(ui.pointer_button(true));
    assert_eq!(ui.scroll_offset(view)[1], 0.0, "pressed at the top of the track");
    ui.pointer_move(197.0, 300.0);
    assert_eq!(ui.scroll_offset(view)[1], 400.0, "dragged past the bottom");
    assert!(ui.pointer_button(false));
    ui.pointer_move(500.0, 500.0);
    assert!(!ui.pointer_scroll(0.0, -50.0), "outside the view the wheel goes to the game");
}

#[test]
fn lists_keep_only_the_rows_on_screen() {
    let mut ui = Ui::new();
    let root = ui.root();
    let host = ui.panel(root, Style::column().at(0.0, 0.0).width(300.0).height(200.0));
    let made = Rc::new(RefCell::new(0));
    let bound = Rc::new(RefCell::new(Vec::new()));
    let (made_by, bound_by) = (made.clone(), bound.clone());
    let items = ItemFns {
        make: move |ui: &mut Ui, row: NodeId| {
            *made_by.borrow_mut() += 1;
            ui.label(row, "", TextStyle::default());
        },
        bind: move |ui: &mut Ui, row: NodeId, index: usize| {
            bound_by.borrow_mut().push(index);
            let label = ui.children(row)[0];
            ui.set_text(label, &format!("Item {index}"));
        },
    };
    let list = ui.list_view(host, 20.0, 100_000, items);
    let mut style = Style::column().scroll().grow();
    style.layout.size.height = taffy::Dimension::length(200.0);
    ui.set_style(list, style);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(*made.borrow(), 11, "ten rows fill 200 pixels, and one more for a partly shown row");
    assert_eq!(shown_rows(&ui, list).len(), 11);

    ui.set_scroll(list, [0.0, 1_000_000.0]);
    bound.borrow_mut().clear();
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(*made.borrow(), 11, "scrolling reuses rows rather than making more");
    let mut indices = bound.borrow().clone();
    indices.sort_unstable();
    assert_eq!(indices, (50_000..50_011).collect::<Vec<_>>(), "bound to the rows now in sight");
    let rows = shown_rows(&ui, list);
    assert_eq!(ui.rect(rows[0])[1], 0.0, "row 50,000 sits at the top of the view");

    click(&mut ui, rows[2]);
    assert_eq!(ui.selection(list), [50_002]);
    assert!(ui.selection_changed(list));
    ui.set_modifiers(Modifiers { shift: true, control: false });
    click(&mut ui, rows[5]);
    assert_eq!(ui.selection(list), [50_002, 50_003, 50_004, 50_005], "shift extends from the first click");
    ui.set_modifiers(Modifiers { shift: false, control: true });
    click(&mut ui, rows[3]);
    assert_eq!(ui.selection(list), [50_002, 50_004, 50_005], "control toggles one");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(!ui.selection_changed(list), "a frame forgets the change");

    ui.set_list_count(list, 10);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(ui.selection(list).is_empty(), "rows that went are deselected");
    assert_eq!(ui.scroll_offset(list)[1], 0.0, "and the scroll comes back within the list");
}

/// Three folders of two files each: ids 1 to 3, and 10 times those plus 1 and 2.
struct Folders;

impl TreeItems for Folders {
    fn children(&self, parent: Option<u64>) -> Vec<u64> {
        match parent {
            None => vec![1, 2, 3],
            Some(folder @ 1..=3) => vec![folder * 10 + 1, folder * 10 + 2],
            Some(_) => Vec::new(),
        }
    }

    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        ui.label(row, "", TextStyle::default());
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let label = ui.children(row)[0];
        ui.set_text(label, &item.to_string());
    }
}

#[test]
fn trees_open_and_close_and_keep_their_selection() {
    let mut ui = Ui::new();
    let root = ui.root();
    let host = ui.panel(root, Style::column().at(0.0, 0.0).width(300.0).height(400.0));
    let tree = ui.tree_view(host, 20.0, Folders);
    ui.set_style(tree, Style::column().scroll().grow());
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(shown_rows(&ui, tree).len(), 3, "closed, only the folders show");

    let second = shown_rows(&ui, tree)[1];
    click(&mut ui, second);
    assert_eq!(ui.selected_items(tree), [2]);
    let arrow = ui.children(shown_rows(&ui, tree)[0])[1];
    click(&mut ui, arrow);
    assert!(ui.is_expanded(tree, 1), "the arrow opens the folder");
    ui.frame(WIDTH, HEIGHT, 1.0);
    let rows = shown_rows(&ui, tree);
    assert_eq!(rows.len(), 5);
    assert_eq!(ui.selected_items(tree), [2], "selection follows the item, not the row");
    let file = ui.children(rows[1])[2];
    assert!(ui.rect(file)[0] > ui.rect(ui.children(rows[0])[2])[0], "files indent under their folder");
    assert!(ui.has_class(ui.children(rows[0])[1], "expanded"));

    ui.set_expanded(tree, 1, false);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(shown_rows(&ui, tree).len(), 3);
}

#[test]
fn tree_rows_drag_onto_one_another_and_context_clicks_select() {
    let mut ui = Ui::new();
    let root = ui.root();
    let host = ui.panel(root, Style::column().at(0.0, 0.0).width(300.0).height(400.0));
    let tree = ui.tree_view(host, 20.0, Folders);
    ui.set_style(tree, Style::column().scroll().grow());
    ui.set_rows_draggable(tree, true);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let rows = shown_rows(&ui, tree);
    let centre = |ui: &Ui, row: NodeId| {
        let [x, y, w, h] = ui.rect(row);
        (x + w / 2.0, y + h / 2.0)
    };

    // Pressed on the third folder, moved a little, then onto the first.
    let (x, y) = centre(&ui, rows[2]);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_move(x + 2.0, y);
    assert!(!ui.has_class(tree, "dragging-rows"), "a small move is still a click");
    let (to_x, to_y) = centre(&ui, rows[0]);
    ui.pointer_move(to_x, to_y);
    assert!(ui.has_class(tree, "dragging-rows"));
    assert!(ui.has_class(rows[0], "drop-target"), "the row it would land on is marked");
    assert!(ui.pointer_button(false));
    assert_eq!(ui.dropped_item(tree), Some((3, Some((1, DropPlace::Onto)))));
    assert_eq!(ui.dropped(tree), Some((2, Some((0, DropPlace::Onto)))));
    assert!(ui.selected_items(tree).is_empty(), "a drag is no click, so nothing was selected");
    assert!(!ui.has_class(rows[0], "drop-target") && !ui.has_class(tree, "dragging-rows"));
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.dropped(tree), None, "drops last a frame");

    // Let go below the rows, it lands on nothing.
    let (x, y) = centre(&ui, rows[1]);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_move(x, 300.0);
    ui.pointer_button(false);
    assert_eq!(ui.dropped_item(tree), Some((2, None)));

    // Near a row's top or bottom edge, it lands before or after the row, which shows a line there.
    for (edge, place, class) in [(0.1, DropPlace::Before, "drop-before"), (0.9, DropPlace::After, "drop-after")] {
        ui.frame(WIDTH, HEIGHT, 1.0);
        let (x, y) = centre(&ui, rows[2]);
        let [_, top, _, height] = ui.rect(rows[0]);
        ui.pointer_move(x, y);
        ui.pointer_button(true);
        ui.pointer_move(x, top + height * edge);
        assert!(ui.has_class(rows[0], class), "{class}");
        ui.pointer_button(false);
        assert_eq!(ui.dropped_item(tree), Some((3, Some((1, place)))));
    }

    // A context click selects the row under it first.
    let (x, y) = centre(&ui, rows[1]);
    ui.pointer_move(x, y);
    ui.pointer_secondary(true);
    ui.pointer_secondary(false);
    assert!(ui.context_clicked(rows[1]) && ui.context_clicked(tree));
    assert_eq!(ui.selected_items(tree), [2]);
}

#[test]
fn rows_and_elements_drop_onto_elements_that_take_them() {
    let mut ui = Ui::new();
    let root = ui.root();
    let host = ui.panel(root, Style::column().at(0.0, 0.0).width(300.0).height(200.0));
    let tree = ui.tree_view(host, 20.0, Folders);
    ui.set_style(tree, Style::column().scroll().grow());
    ui.set_rows_draggable(tree, true);
    let card = ui.panel(root, Style::default().at(0.0, 300.0).width(100.0).height(40.0));
    ui.label(card, "Card", TextStyle::default());
    ui.set_draggable(card, true);
    let field = ui.panel(root, Style::default().at(400.0, 100.0).width(200.0).height(40.0));
    ui.accept_drops(field, true);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let centre = |ui: &Ui, node: NodeId| {
        let [x, y, w, h] = ui.rect(node);
        (x + w / 2.0, y + h / 2.0)
    };

    // A tree's row, dragged over the field: it is marked, and a ghost of the row follows.
    let row = shown_rows(&ui, tree)[1];
    let (x, y) = centre(&ui, row);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    let (fx, fy) = centre(&ui, field);
    ui.pointer_move(fx, fy);
    assert!(ui.has_class(field, "drop-hover"));
    assert!(matches!(ui.dragging(), Some(voxear_ui::DragSource::Row { item: Some(2), .. })));
    ui.frame(WIDTH, HEIGHT, 1.0);
    let layer = *ui.children(ui.root()).last().unwrap();
    let ghost = ui.children(layer)[0];
    assert!(
        ui.has_class(ghost, "drag-ghost") && ui.text(ui.children(ghost)[0]) == "2",
        "the ghost says what is dragged"
    );
    assert!(ui.pointer_button(false));
    assert_eq!(ui.dropped_onto(field), Some(voxear_ui::DragSource::Row { list: tree, index: 1, item: Some(2) }));
    assert_eq!(ui.dropped(tree), None, "a drop onto the field is not one in the list");
    assert!(!ui.has_class(field, "drop-hover") && ui.dragging().is_none());

    // An element made draggable drops the same way.
    ui.frame(WIDTH, HEIGHT, 1.0);
    let (x, y) = centre(&ui, card);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_move(fx, fy);
    ui.pointer_button(false);
    assert_eq!(ui.dropped_onto(field), Some(voxear_ui::DragSource::Element(card)));
    assert!(!ui.clicked(card), "a drag is no click");
}

#[test]
fn a_tree_whose_rows_take_drops_takes_them_from_elsewhere() {
    let mut ui = Ui::new();
    let root = ui.root();
    let host = ui.panel(root, Style::column().at(0.0, 0.0).width(300.0).height(200.0));
    let tree = ui.tree_view(host, 20.0, Folders);
    ui.set_style(tree, Style::column().scroll().grow());
    ui.set_rows_draggable(tree, true);
    ui.accept_row_drops(tree, true);
    let card = ui.panel(root, Style::default().at(0.0, 300.0).width(100.0).height(40.0));
    ui.label(card, "Card", TextStyle::default());
    ui.set_draggable(card, true);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let rows = shown_rows(&ui, tree);
    let centre = |ui: &Ui, node: NodeId| {
        let [x, y, w, h] = ui.rect(node);
        (x + w / 2.0, y + h / 2.0)
    };

    // The card, held over the second row, marks it, and lands on its item.
    let (x, y) = centre(&ui, card);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    let (to_x, to_y) = centre(&ui, rows[1]);
    ui.pointer_move(to_x, to_y);
    assert!(ui.has_class(rows[1], "drop-target"), "the row it would land on is marked");
    ui.pointer_button(false);
    assert_eq!(ui.dropped_on_item(tree), Some((voxear_ui::DragSource::Element(card), Some((2, DropPlace::Onto)))));
    assert!(!ui.has_class(rows[1], "drop-target"));

    // Near a row's edge it lands beside it, and below the rows on nothing.
    ui.frame(WIDTH, HEIGHT, 1.0);
    let [_, top, _, height] = ui.rect(rows[0]);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_move(to_x, top + height * 0.9);
    assert!(ui.has_class(rows[0], "drop-after"));
    ui.pointer_button(false);
    assert_eq!(ui.dropped_on_item(tree), Some((voxear_ui::DragSource::Element(card), Some((1, DropPlace::After)))));
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_move(to_x, 180.0);
    ui.pointer_button(false);
    assert_eq!(ui.dropped_on_item(tree), Some((voxear_ui::DragSource::Element(card), None)));

    // Its own rows still land in it as rows, not as drops from elsewhere.
    ui.frame(WIDTH, HEIGHT, 1.0);
    let (x, y) = centre(&ui, rows[2]);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    let (to_x, to_y) = centre(&ui, rows[0]);
    ui.pointer_move(to_x, to_y);
    ui.pointer_button(false);
    assert_eq!(ui.dropped_item(tree), Some((3, Some((1, DropPlace::Onto)))));
    assert_eq!(ui.dropped_on_item(tree), None);
    assert_eq!(ui.dropped_onto(tree), None);
}

#[test]
fn lists_declared_in_markup_get_their_items_from_code() {
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"<vui><ListView name="log" item-height="18" style="height: 180px" /></vui>"#;
    let document = ui.load_markup(root, "Log.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let log = ui.find(document, "log").unwrap();
    let items = ItemFns {
        make: |ui: &mut Ui, row: NodeId| {
            ui.label(row, "", TextStyle::default());
        },
        bind: |ui: &mut Ui, row: NodeId, index: usize| {
            let label = ui.children(row)[0];
            ui.set_text(label, &format!("Line {index}"));
        },
    };
    ui.bind_list(log, 1000, items);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(shown_rows(&ui, log).len(), 11, "18-pixel rows in 180 pixels");
    assert!(ui.take_warnings().is_empty());
}

#[test]
fn arrow_keys_move_through_lists_and_open_and_close_trees() {
    use voxear_ui::Key;
    let mut ui = Ui::new();
    let root = ui.root();
    let host = ui.panel(root, Style::column().at(0.0, 0.0).width(300.0).height(100.0));
    let items = ItemFns {
        make: |ui: &mut Ui, row: NodeId| {
            ui.label(row, "", TextStyle::default());
        },
        bind: |ui: &mut Ui, row: NodeId, index: usize| {
            let label = ui.children(row)[0];
            ui.set_text(label, &format!("Item {index}"));
        },
    };
    let list = ui.list_view(host, 20.0, 50, items);
    ui.set_style(list, Style::column().scroll().grow());
    ui.frame(WIDTH, HEIGHT, 1.0);

    // A click on a row gives the list the keys.
    let second = shown_rows(&ui, list)[1];
    click(&mut ui, second);
    assert_eq!(ui.focused(), Some(list), "the list has focus");
    assert!(ui.key_down(Key::Down));
    assert_eq!(ui.selection(list), [2]);
    assert!(ui.selection_changed(list), "as a click's change does");
    ui.key_down(Key::Up);
    ui.key_down(Key::Up);
    ui.key_down(Key::Up);
    assert_eq!(ui.selection(list), [0], "and stops at the first");

    // Shift extends from where the selection began; Home, End and the page keys jump.
    ui.set_modifiers(Modifiers { shift: true, control: false });
    ui.key_down(Key::Down);
    ui.key_down(Key::Down);
    assert_eq!(ui.selection(list), [0, 1, 2]);
    ui.set_modifiers(Modifiers::default());
    ui.key_down(Key::End);
    assert_eq!(ui.selection(list), [49]);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let rows = shown_rows(&ui, list);
    let last = rows.last().copied().unwrap();
    assert_eq!(ui.text(ui.children(last)[0]), "Item 49", "scrolled to show it");
    ui.key_down(Key::PageUp);
    assert_eq!(ui.selection(list), [44], "a screenful of five rows up");
    ui.key_down(Key::Home);
    assert_eq!(ui.selection(list), [0]);
    assert!(!ui.key_down(Key::Left), "a list leaves Left and Right to others");

    // In a tree, Right opens, then goes in; Left goes back out, then closes.
    let beside = ui.panel(root, Style::column().at(400.0, 0.0).width(300.0).height(200.0));
    let tree = ui.tree_view(beside, 20.0, Folders);
    ui.set_style(tree, Style::column().scroll().grow());
    ui.frame(WIDTH, HEIGHT, 1.0);
    let first = shown_rows(&ui, tree)[0];
    let label = ui.children(first)[2];
    click(&mut ui, label);
    assert_eq!((ui.focused(), ui.selected_items(tree)), (Some(tree), vec![1]));
    ui.key_down(Key::Right);
    assert!(ui.is_expanded(tree, 1), "Right opens it");
    ui.key_down(Key::Right);
    assert_eq!(ui.selected_items(tree), [11], "and then goes to the first inside");
    ui.key_down(Key::Down);
    assert_eq!(ui.selected_items(tree), [12]);
    ui.key_down(Key::Left);
    assert_eq!(ui.selected_items(tree), [1], "Left goes to what it is in");
    ui.key_down(Key::Left);
    assert!(!ui.is_expanded(tree, 1), "and then closes it");
    ui.key_down(Key::Down);
    assert_eq!(ui.selected_items(tree), [2]);
}

#[test]
fn an_element_can_take_the_wheel_rather_than_scroll() {
    let mut ui = Ui::new();
    let root = ui.root();
    let scroller = ui.panel(root, Style::column().scroll().at(0.0, 0.0).width(200.0).height(100.0));
    let canvas = ui.panel(scroller, Style::default().width(200.0).height(400.0));
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.pointer_move(50.0, 50.0);
    assert!(ui.pointer_scroll(0.0, 30.0));
    assert_eq!(ui.scroll_offset(scroller), [0.0, 30.0], "without taking it, the wheel scrolls");

    ui.take_wheel(canvas, true);
    assert!(ui.pointer_scroll(0.0, -20.0));
    assert!(ui.pointer_scroll(0.0, -5.0));
    assert_eq!(ui.wheeled(canvas), [0.0, -25.0], "the turns since the last frame");
    assert_eq!(ui.scroll_offset(scroller), [0.0, 30.0], "and nothing scrolls");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.wheeled(canvas), [0.0, 0.0], "a frame forgets them");
}
