//! Rich text: spans, and links that can be clicked, without a GPU.

use voxear_ui::{Marks, RichText, Style, Ui};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

#[test]
fn links_in_rich_text_report_their_clicks() {
    let mut ui = Ui::new();
    let root = ui.root();
    // In a row, so the label is as wide as its words.
    let box_ = ui.panel(root, Style::row().at(0.0, 0.0));
    let mut text = RichText::new();
    text.push("Read ", Marks::default())
        .push("the guide", Marks { strong: true, ..Marks::default() })
        .push(" or open ", Marks::default())
        .push_link("Writing Pages", "docs:Writing Pages", Marks::default())
        .push(" now.", Marks::default());
    let label = ui.rich_label(box_, &text);
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(frame.quads.len() > 20, "the words are drawn");
    let [x, y, w, h] = ui.rect(label);
    assert!(w > 100.0 && h > 10.0, "it is laid out like any label: {w} by {h}");

    // The middle of "Writing Pages", by its share of the characters.
    let middle = (["Read ", "the guide", " or open "].concat().len() + 6) as f32 / text.text.len() as f32;
    ui.pointer_move(x + w * middle, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
    assert_eq!(ui.clicked_link(box_), Some("docs:Writing Pages"), "the link and what it is in report it");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.clicked_link(label), None, "a frame forgets it");

    ui.pointer_move(x + 4.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
    assert!(ui.clicked(label) && ui.clicked_link(label).is_none(), "plain words are no link");

    ui.set_text(label, "Plain again");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.text(label), "Plain again");
}
