//! A project's own fonts: text styled with one draws in it once its data
//! is given, and text in a language that names fonts falls back to them for
//! the letters its own font has not got. `fonts/Wide.ttf` is a font made
//! for this test, each of its letters (A, B, 日 and ب) three ems wide.

use std::sync::Arc;

use voxear_ui::{Style, StyleSheet, Ui, fonts};

const WIDE: &[u8] = include_bytes!("fonts/Wide.ttf");

fn width(ui: &Ui, node: voxear_ui::NodeId) -> f32 {
    ui.rect(node)[2]
}

#[test]
fn text_styled_with_a_projects_font_draws_in_it_once_given() {
    let mut ui = Ui::bare();
    let root = ui.root();
    let sheet = ".title { font-family: url(\"Fonts/Wide Title.ttf\"); font-size: 20px; }\n";
    ui.add_sheet(root, StyleSheet::parse("Fonts.vss", sheet));
    let row = ui.panel(root, Style::row());
    let label = ui.add_label(row, "AB");
    ui.add_class(label, "title");
    ui.frame(800, 200, 1.0);
    let before = width(&ui, label);
    assert!(before < 40.0, "in Voxear's face before the font is read: {before}");

    let face = fonts::register(&["Fonts/Wide Title.ttf", "5e1a0c2b"], Arc::new(WIDE.to_vec())).unwrap();
    assert!(face.is_loaded());
    ui.frame(800, 200, 1.0);
    let after = width(&ui, label);
    assert!((after - 120.0).abs() < 1.0, "two letters three ems wide at 20 px: {after}");

    // A UI made after the font was given has it too.
    let mut late = ui.sharing_fonts();
    let root = late.root();
    late.clear_sheets(root);
    late.add_sheet(root, StyleSheet::parse("Fonts.vss", ".title { font-family: url(\"5e1a0c2b\"); font-size: 10px; }"));
    let row = late.panel(root, Style::row());
    let label = late.add_label(row, "A");
    late.add_class(label, "title");
    late.frame(800, 200, 1.0);
    assert!((width(&late, label) - 30.0).abs() < 1.0, "named by its GUID: {}", width(&late, label));
}

#[test]
fn a_languages_fonts_draw_the_letters_an_elements_own_font_has_not_got() {
    let face = fonts::register(&["Fonts/Wide Language.ttf"], Arc::new(WIDE.to_vec())).unwrap();
    let mut ui = Ui::bare();
    let root = ui.root();
    ui.add_sheet(root, StyleSheet::parse("Size.vss", ".small { font-size: 10px; }"));
    let row = ui.panel(root, Style::row());
    let label = ui.add_label(row, "日");
    ui.add_class(label, "small");
    ui.frame(800, 200, 1.0);
    let system = width(&ui, label);
    assert!(system < 20.0, "the system's own CJK face, or none, at first: {system}");

    ui.set_language_fonts(&[face]);
    assert_eq!(ui.language_fonts(), &[face]);
    ui.frame(800, 200, 1.0);
    assert!((width(&ui, label) - 30.0).abs() < 1.0, "the language's font, before Voxear's: {}", width(&ui, label));

    ui.set_language_fonts(&[]);
    ui.frame(800, 200, 1.0);
    assert!((width(&ui, label) - system).abs() < 0.5, "and back");
}
