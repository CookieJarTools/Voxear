//! A UI laid out right to left, as Arabic and Hebrew read: a row runs from
//! the right, a margin on the left moves to the right, and a part styled
//! `direction: ltr` keeps its own way round.

use voxear_ui::{Style, StyleSheet, Ui};

#[test]
fn a_right_to_left_ui_mirrors_its_rows_and_margins_but_not_what_keeps_left_to_right() {
    let mut ui = Ui::new();
    let root = ui.root();
    let sheet = ".gap { margin-left: 30px; width: 20px; height: 20px; }\n.kept { direction: ltr; }\n";
    ui.add_sheet(root, StyleSheet::parse("Mirror.vss", sheet));
    let row = ui.panel(root, Style::row());
    let first = ui.panel(row, Style::default());
    ui.add_class(first, "gap");
    let second = ui.add_label(row, "Second");
    let kept = ui.panel(root, Style::row());
    ui.add_class(kept, "kept");
    let left = ui.add_label(kept, "Left");
    let right = ui.add_label(kept, "Right");
    ui.frame(400, 200, 1.0);
    let x = |ui: &Ui, node| ui.rect(node)[0];
    assert!(x(&ui, first) < x(&ui, second));
    assert_eq!(x(&ui, first), 30.0);

    ui.set_right_to_left(true);
    ui.frame(400, 200, 1.0);
    assert!(ui.is_right_to_left());
    assert!(x(&ui, first) > x(&ui, second), "the row runs from the right");
    assert_eq!(x(&ui, first), 400.0 - 30.0 - 20.0, "its margin on its right");
    assert!(ui.text_style(second).unwrap().rtl, "text in it starts at the right");
    assert!(x(&ui, left) < x(&ui, right), "a part styled ltr keeps its way round");

    ui.set_right_to_left(false);
    ui.frame(400, 200, 1.0);
    assert_eq!(x(&ui, first), 30.0, "and back");
}
