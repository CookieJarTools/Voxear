//! Reordering a list's items by their handles, driven through the pointer
//! as a user would, without a GPU.

use voxear_ui::{NodeId, Style, Ui};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;
const FRAME: f32 = 1.0 / 60.0;

/// Four items 30 pixels tall with 4 between them, each with a handle.
fn list() -> (Ui, NodeId, Vec<NodeId>, Vec<NodeId>) {
    let mut ui = Ui::new();
    let root = ui.root();
    let list = ui.panel(root, Style::column());
    ui.set_inline_style(list, "position: absolute; left: 10px; top: 10px; width: 200px; gap: 4px");
    ui.set_reorderable(list, true);
    let (mut items, mut handles) = (Vec::new(), Vec::new());
    for name in ["Zero", "One", "Two", "Three"] {
        let item = ui.panel(list, Style::row());
        ui.set_inline_style(item, "height: 30px");
        let handle = ui.icon(item, "grip");
        ui.set_inline_style(handle, "width: 16px; height: 16px");
        ui.set_reorder_handle(handle, true);
        ui.add_label(item, name);
        items.push(item);
        handles.push(handle);
    }
    ui.frame(WIDTH, HEIGHT, 1.0);
    (ui, list, items, handles)
}

fn centre(ui: &Ui, node: NodeId) -> (f32, f32) {
    let [x, y, w, h] = ui.rect(node);
    (x + w / 2.0, y + h / 2.0)
}

fn frames(ui: &mut Ui, count: usize) {
    for _ in 0..count {
        ui.advance(FRAME);
        ui.frame(WIDTH, HEIGHT, 1.0);
    }
}

#[test]
fn an_item_dragged_by_its_handle_lifts_the_others_slide_and_it_settles() {
    let (mut ui, list, items, handles) = list();
    assert!(ui.take_warnings().is_empty(), "the grip icon exists");
    let from = centre(&ui, handles[0]);
    ui.pointer_move(from.0, from.1);
    ui.pointer_button(true);
    ui.pointer_move(from.0, from.1 + 2.0);
    assert_eq!(ui.reordering(), None, "a few pixels are not a drag yet");

    // Two items down, it follows the pointer, lifted, and the two it passed slide up to make room.
    ui.pointer_move(from.0 + 30.0, from.1 + 70.0);
    assert_eq!(ui.reordering(), Some(list));
    frames(&mut ui, 30);
    assert!(ui.has_class(items[0], "reorder-lifted"));
    assert_eq!(ui.slide(items[0]), 70.0, "on the pointer");
    assert!((ui.slide(items[1]) + 34.0).abs() < 0.5 && (ui.slide(items[2]) + 34.0).abs() < 0.5);
    assert_eq!(ui.slide(items[3]), 0.0, "the last stays");

    // Let go, the move is reported, and each item eases from where it was drawn.
    ui.pointer_button(false);
    assert_eq!(ui.reordered(list), Some((0, 2)));
    assert!(!ui.has_class(items[0], "reorder-lifted"));
    assert!((ui.slide(items[2]) - 2.0).abs() < 0.5, "the third shows the dragged item, 2 below its place");
    frames(&mut ui, 1);
    assert_eq!(ui.reordered(list), None, "reported once");
    frames(&mut ui, 30);
    assert!(items.iter().all(|&item| ui.slide(item) == 0.0), "settled");
}

#[test]
fn a_click_on_a_handle_moves_nothing_and_a_drag_stays_within_the_list() {
    let (mut ui, list, items, handles) = list();
    let at = centre(&ui, handles[1]);
    ui.pointer_move(at.0, at.1);
    ui.pointer_button(true);
    ui.pointer_button(false);
    assert!(ui.clicked(handles[1]) && ui.reordered(list).is_none());

    // Far above the list, it stops at the top, and lands first.
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.pointer_move(at.0, at.1);
    ui.pointer_button(true);
    ui.pointer_move(at.0, at.1 - 200.0);
    frames(&mut ui, 2);
    assert_eq!(ui.slide(items[1]), -34.0, "no higher than the first item's place");
    ui.pointer_button(false);
    assert_eq!(ui.reordered(list), Some((1, 0)));
}
