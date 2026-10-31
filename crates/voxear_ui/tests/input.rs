//! Focus, the keyboard and the input controls, without a GPU.

use voxear_ui::{Key, Modifiers, NodeId, Quad, Style, Ui};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

fn with(ui: &mut Ui, shift: bool, control: bool, key: Key) -> bool {
    ui.set_modifiers(Modifiers { shift, control });
    let used = ui.key_down(key);
    ui.set_modifiers(Modifiers::default());
    used
}

fn form() -> (Ui, NodeId, NodeId, NodeId, NodeId) {
    let mut ui = Ui::new();
    let root = ui.root();
    let form = ui.panel(root, Style::column().at(10.0, 10.0).width(240.0).gap(8.0));
    let field = ui.text_field(form, "");
    ui.set_placeholder(field, "Name");
    let button = ui.button(form, "Save");
    let toggle = ui.toggle(form, "Snap to grid", false);
    let slider = ui.slider(form, 0.0, 10.0, 5.0);
    ui.frame(WIDTH, HEIGHT, 1.0);
    (ui, field, button, toggle, slider)
}

#[test]
fn text_fields_take_typing_editing_and_the_clipboard() {
    let (mut ui, field, ..) = form();
    assert!(!ui.text_input("ignored"), "nothing has focus");
    click(&mut ui, field);
    assert_eq!(ui.focused(), Some(field));
    assert!(ui.wants_keyboard());

    assert!(ui.text_input("hello world"));
    assert_eq!(ui.text(field), "hello world");
    assert!(ui.changed(field));
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(!ui.changed(field), "a frame forgets the change");

    with(&mut ui, false, true, Key::Backspace);
    assert_eq!(ui.text(field), "hello ", "Ctrl+Backspace takes a word");
    with(&mut ui, false, false, Key::Home);
    with(&mut ui, true, false, Key::End);
    with(&mut ui, false, true, Key::Char('c'));
    with(&mut ui, false, false, Key::End);
    with(&mut ui, false, true, Key::Char('v'));
    assert_eq!(ui.text(field), "hello hello ", "copied the selection and pasted it at the end");
    with(&mut ui, false, true, Key::Char('a'));
    with(&mut ui, false, true, Key::Char('x'));
    assert_eq!(ui.text(field), "", "cut everything");

    ui.text_input("line\nbreak");
    assert_eq!(ui.text(field), "linebreak", "one line drops newlines");
    assert!(with(&mut ui, false, false, Key::Enter));
    assert!(ui.submitted(field));
    assert!(with(&mut ui, false, false, Key::Char('w')), "keys stay with the field, away from the game");
    with(&mut ui, false, false, Key::Escape);
    assert_eq!(ui.focused(), None);
    assert!(!ui.key_down(Key::Char('w')), "and once it lets go, they go to the game");

    ui.set_text(field, "set from code");
    assert_eq!(ui.text(field), "set from code");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(!ui.changed(field), "setting from code is not a change by the user");
}

#[test]
fn double_click_selects_a_word_and_long_text_scrolls() {
    let (mut ui, field, ..) = form();
    ui.set_text(field, "alpha beta");
    ui.frame(WIDTH, HEIGHT, 1.0);
    let [x, y, _, h] = ui.rect(field);
    ui.pointer_move(x + 12.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
    ui.advance(0.1);
    ui.pointer_button(true);
    ui.pointer_button(false);
    with(&mut ui, false, true, Key::Char('x'));
    assert_eq!(ui.text(field), " beta", "the double click took 'alpha'");

    ui.text_input(&"long ".repeat(40));
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    let [fx, fy, fw, fh] = ui.rect(field);
    let caret = frame.quads.iter().find(|q| q.kind == Quad::SHAPE && q.rect[2] <= 2.0 && q.rect[3] > 8.0);
    let caret = caret.expect("the focused field draws its caret");
    assert!(caret.rect[0] > fx && caret.rect[0] < fx + fw, "the text scrolled to keep the caret in the field");
    let in_field = |q: &&Quad| q.kind == Quad::GLYPH && q.rect[1] >= fy && q.rect[1] < fy + fh;
    let glyphs: Vec<&Quad> = frame.quads.iter().filter(in_field).collect();
    assert!(glyphs.len() > 100, "the long text's glyphs, before clipping");
    assert!(glyphs.iter().all(|g| g.clip[0] >= fx && g.clip[2] <= fx + fw), "text is clipped to the field");
}

#[test]
fn tab_walks_the_controls_and_keys_work_them() {
    let (mut ui, field, button, toggle, slider) = form();
    assert!(ui.key_down(Key::Tab));
    assert_eq!(ui.focused(), Some(field));
    ui.key_down(Key::Tab);
    assert_eq!(ui.focused(), Some(button));
    assert!(ui.key_down(Key::Enter));
    assert!(ui.clicked(button), "Enter presses a focused button");
    ui.key_down(Key::Tab);
    assert_eq!(ui.focused(), Some(toggle));
    ui.key_down(Key::Space);
    assert!(ui.is_checked(toggle) && ui.changed(toggle));
    ui.key_down(Key::Tab);
    assert_eq!(ui.focused(), Some(slider));
    ui.key_down(Key::Right);
    assert_eq!(ui.slider_value(slider), 5.1, "a hundredth of the range");
    ui.key_down(Key::End);
    assert_eq!(ui.slider_value(slider), 10.0);
    ui.key_down(Key::Tab);
    assert_eq!(ui.focused(), Some(field), "Tab wraps round");
    with(&mut ui, true, false, Key::Tab);
    assert_eq!(ui.focused(), Some(slider), "Shift+Tab goes back");

    ui.set_enabled(button, false);
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.key_down(Key::Tab);
    ui.key_down(Key::Tab);
    assert_eq!(ui.focused(), Some(toggle), "a disabled button is skipped");
}

#[test]
fn sliders_drag_and_snap_toggles_click_foldouts_fold() {
    let (mut ui, _, _, toggle, slider) = form();
    ui.set_slider_step(slider, 2.5);
    let [x, y, w, h] = ui.rect(slider);
    ui.pointer_move(x + w * 0.7, y + h / 2.0);
    ui.pointer_button(true);
    assert_eq!(ui.slider_value(slider), 7.5, "7 snaps to the nearest step");
    ui.pointer_move(x + w * 2.0, y + h / 2.0);
    assert_eq!(ui.slider_value(slider), 10.0, "dragged past the end");
    ui.pointer_button(false);
    assert!(ui.changed(slider));

    click(&mut ui, toggle);
    assert!(ui.is_checked(toggle));
    click(&mut ui, toggle);
    assert!(!ui.is_checked(toggle));

    let root = ui.root();
    let foldout = ui.foldout(root, "Movement", true);
    let content = ui.foldout_content(foldout).unwrap();
    ui.label(content, "Speed", Default::default());
    ui.set_style(foldout, Style::column().at(400.0, 10.0).width(200.0));
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(ui.is_open(foldout) && ui.is_visible(content));
    let header = ui.children(foldout)[0];
    click(&mut ui, header);
    assert!(!ui.is_open(foldout) && !ui.is_visible(content) && ui.changed(foldout));
    click(&mut ui, content);
    assert!(!ui.is_open(foldout), "clicking a closed foldout's hidden content does nothing");
}

#[test]
fn controls_come_from_markup_with_the_default_theme() {
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"
        <vui>
            <Foldout text="Movement" open="true" name="movement">
                <TextField name="name" placeholder="Name" />
                <Toggle name="fly" value="true">Can fly</Toggle>
                <Slider name="speed" min="0" max="20" value="6" step="0.5" />
            </Foldout>
        </vui>"#;
    let document = ui.load_markup(root, "Form.vui", markup, &mut |_| Err("no files".into())).unwrap();
    ui.frame(WIDTH, HEIGHT, 1.0);
    let find = |name: &str| ui.find(document, name).unwrap();
    assert!(ui.is_checked(find("fly")));
    assert_eq!(ui.slider_value(find("speed")), 6.0);
    assert!(ui.is_open(find("movement")));
    assert_eq!(ui.rect(find("name"))[3], 24.0, "the theme's field height");
    assert!(ui.take_warnings().is_empty(), "the theme and the markup are clean");
}

#[test]
fn a_multiline_field_scrolls_to_its_caret_and_with_the_wheel() {
    let mut ui = Ui::new();
    let root = ui.root();
    let field = ui.text_field(root, "");
    ui.set_style(field, Style::column().at(10.0, 10.0).width(300.0));
    // Inline, since the theme gives text fields a line's height.
    ui.set_inline_style(field, "height: 100px");
    ui.set_multiline(field, true);
    ui.frame(WIDTH, HEIGHT, 1.0);
    click(&mut ui, field);
    for line in 0..30 {
        ui.text_input(&format!("line {line}"));
        ui.key_down(Key::Enter);
    }
    let caret = |quads: &[Quad]| {
        let caret = quads.iter().find(|q| q.kind == Quad::SHAPE && q.rect[2] <= 2.0 && q.rect[3] > 8.0);
        caret.map(|q| q.rect)
    };
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    let [fx, fy, fw, fh] = ui.rect(field);
    let [_, top, _, height] = caret(&frame.quads).expect("the focused field draws its caret");
    assert!(top >= fy && top + height <= fy + fh + 1.0, "the text scrolled to keep the caret in the field");
    let thumb = |q: &&Quad| q.kind == Quad::SHAPE && q.rect[2] > 3.0 && q.rect[2] < 10.0 && q.rect[0] > fx + fw - 12.0;
    assert!(frame.quads.iter().any(|q| thumb(&q)), "a thumb shows there is more above");

    // The wheel goes back to the top, leaving the caret where it is.
    ui.pointer_move(fx + 40.0, fy + 40.0);
    assert!(ui.pointer_scroll(0.0, -10_000.0), "the field took the wheel");
    assert!(!ui.pointer_scroll(0.0, -10.0), "and can go no further up");
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(caret(&frame.quads).is_none_or(|[_, top, ..]| top >= fy + fh), "the caret is out of sight below");
    let first = frame.quads.iter().filter(|q| q.kind == Quad::GLYPH).map(|q| q.rect[1]).fold(f32::MAX, f32::min);
    assert!(first < fy + 20.0, "the first line shows at the top");

    // Page Up from the end climbs a field's height, and Ctrl+Home goes to the very start.
    with(&mut ui, false, true, Key::End);
    with(&mut ui, false, false, Key::PageUp);
    with(&mut ui, false, true, Key::Home);
    ui.text_input("Top. ");
    assert!(ui.text(field).starts_with("Top. line 0"));
}

#[test]
fn text_fields_undo_a_word_at_a_time_and_redo() {
    let (mut ui, field, ..) = form();
    click(&mut ui, field);
    for letter in "one two".chars() {
        ui.text_input(&letter.to_string());
    }
    with(&mut ui, false, false, Key::Backspace);
    assert_eq!(ui.text(field), "one tw");
    let undo = |ui: &mut Ui| with(ui, false, true, Key::Char('z'));
    undo(&mut ui);
    assert_eq!(ui.text(field), "one two", "the deletion first");
    undo(&mut ui);
    assert_eq!(ui.text(field), "one ", "then the last word");
    undo(&mut ui);
    assert_eq!(ui.text(field), "", "then the first, with its space");
    undo(&mut ui);
    assert_eq!(ui.text(field), "", "and nothing more");

    with(&mut ui, true, true, Key::Char('z'));
    assert_eq!(ui.text(field), "one ");
    with(&mut ui, false, true, Key::Char('y'));
    assert_eq!(ui.text(field), "one two");
    ui.text_input("!");
    with(&mut ui, false, true, Key::Char('y'));
    assert_eq!(ui.text(field), "one two!", "typing after an undo leaves nothing to redo");

    ui.set_text(field, "set from code");
    undo(&mut ui);
    assert_eq!(ui.text(field), "set from code", "text set from code starts the history again");
}
