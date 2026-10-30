//! Scroll, list and tree views, without a GPU.

use std::{cell::RefCell, rc::Rc};

use voxear_ui::{ItemFns, Modifiers, NodeId, Quad, Style, TextStyle, TreeItems, Ui};

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
