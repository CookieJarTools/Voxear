//! Prompts, without a GPU: each glyph an element of its shape's class with
//! its text or icon, made again only when the glyphs change, and the action
//! named in markup or set as its text.

use voxear_ui::{Color, GlyphShape, PromptGlyph, Style, Ui};

#[test]
fn a_prompt_shows_its_glyphs_and_keeps_them_until_they_change() {
    let mut ui = Ui::new();
    let root = ui.root();
    let holder = ui.panel(root, Style::cover());
    let prompt = ui.prompt(holder, "Jump");
    assert_eq!(ui.prompts(), [(prompt, "Jump".to_string())]);

    let space = PromptGlyph { shape: GlyphShape::Key, text: "Space".into(), ..PromptGlyph::default() };
    let green = Color::rgb(0x5d, 0xb5, 0x3a);
    let a = PromptGlyph { shape: GlyphShape::Round, text: "A".into(), color: Some(green), ..PromptGlyph::default() };
    ui.set_prompt(prompt, vec![space.clone()]);
    let glyph = ui.children(prompt)[0];
    assert!(ui.has_class(glyph, "prompt-glyph") && ui.has_class(glyph, "key"));
    assert_eq!(ui.text(ui.children(glyph)[0]), "Space");
    ui.set_prompt(prompt, vec![space]);
    assert_eq!(ui.children(prompt), [glyph], "the same glyphs are left as they are");

    ui.set_prompt(prompt, vec![a]);
    let glyph = ui.children(prompt)[0];
    assert!(ui.has_class(glyph, "round"));
    ui.set_text(prompt, "Player/Crouch");
    assert_eq!(ui.prompt_action(prompt), Some("Player/Crouch"), "a prompt's text is its action");
}

#[test]
fn markup_names_a_prompts_action() {
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"<vui><Panel><Prompt name="hint" action="Jump" /></Panel></vui>"#;
    let document = ui.load_markup(root, "Hint.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let hint = ui.find(document, "hint").unwrap();
    assert!(ui.is_prompt(hint));
    assert_eq!(ui.prompt_action(hint), Some("Jump"));
}
