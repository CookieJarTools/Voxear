//! Style sheets, markup and the pointer, without a GPU.

use std::collections::HashMap;

use voxear_ui::{Color, StyleSheet, TextStyle, Ui};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn background(ui: &Ui, node: voxear_ui::NodeId) -> Option<Color> {
    ui.style(node).unwrap().background
}

/// Loads `markup` from a set of files, as an engine would from a project.
fn load(ui: &mut Ui, markup: &str, files: &[(&str, &str)]) -> Result<voxear_ui::NodeId, voxear_ui::UiError> {
    let files: HashMap<String, String> = files.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let root = ui.root();
    ui.load_markup(root, "Test.vui", markup, &mut |path| files.get(path).cloned().ok_or_else(|| "no such file".into()))
}

const CARDS: &str = r#"
    :root { --accent: #8b93ff; --text: #e8e8ec; }
    Panel { background-color: #111111; }
    .card { background-color: #222222; color: var(--text); font-size: 14px; }
    #main { background-color: #333333; }
    .card.big { background-color: #444444; }
    .list > .card { border: 1px solid var(--accent); }
    .list Label { font-weight: 600; }
    Button:hover { background-color: #555555; }
    Button:active { background-color: #666666; }
    Button:disabled { opacity: 0.4; }
"#;

const DOCUMENT: &str = r#"
<vui>
    <Style src="Cards.vss" />
    <Panel class="list">
        <Panel name="plain" />
        <Panel name="card" class="card"><Label name="caption">Caption</Label></Panel>
        <Panel name="main" class="card big" />
        <Panel name="inline" class="card" style="background-color: #abcdef" />
    </Panel>
    <Button name="go" style="position: absolute; left: 100px; top: 300px; width: 80px; height: 30px">Go</Button>
</vui>
"#;

fn cards() -> (Ui, voxear_ui::NodeId) {
    let mut ui = Ui::new();
    let document = load(&mut ui, DOCUMENT, &[("Cards.vss", CARDS)]).unwrap();
    ui.frame(WIDTH, HEIGHT, 1.0);
    (ui, document)
}

#[test]
fn rules_apply_by_specificity_and_inline_wins() {
    let (ui, document) = cards();
    let find = |name: &str| ui.find(document, name).unwrap();
    assert_eq!(background(&ui, find("plain")), Some(Color::hex(0x111111)), "by type");
    assert_eq!(background(&ui, find("card")), Some(Color::hex(0x222222)), "a class beats a type");
    assert_eq!(background(&ui, find("main")), Some(Color::hex(0x333333)), "a name beats two classes");
    assert_eq!(background(&ui, find("inline")), Some(Color::hex(0xabcdef)), "inline beats every sheet");
    assert_eq!(ui.style(find("card")).unwrap().border, Some((1.0, Color::hex(0x8b93ff))), "a child, with a variable");
}

#[test]
fn a_clean_document_has_nothing_to_report() {
    let (mut ui, _) = cards();
    assert_eq!(ui.take_warnings(), Vec::<String>::new());
}

#[test]
fn text_inherits_and_descendant_rules_reach_it() {
    let (ui, document) = cards();
    let caption = ui.find(document, "caption").unwrap();
    let text = ui.text_style(caption).unwrap();
    assert_eq!(text.color, Color::hex(0xe8e8ec), "colour from the card, through a variable");
    assert_eq!(text.size, 14.0, "size from the card");
    assert_eq!(text.weight, 600.0, "weight from a descendant rule");
    let plain = ui.text_style(ui.find(document, "plain").unwrap()).unwrap();
    assert_eq!(plain, TextStyle::default(), "nothing set, nothing inherited");
}

#[test]
fn the_pointer_hovers_presses_and_clicks() {
    let (mut ui, document) = cards();
    let go = ui.find(document, "go").unwrap();
    assert_eq!(background(&ui, go), Some(Color::hex(0x1a1b20)), "the default theme's button");

    ui.pointer_move(120.0, 310.0);
    assert!(ui.wants_pointer() && ui.is_hovered(go) && ui.is_hovered(document), "the button and what it is in");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(background(&ui, go), Some(Color::hex(0x555555)), ":hover");

    assert!(ui.pointer_button(true), "the UI takes the press");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(background(&ui, go), Some(Color::hex(0x666666)), ":active beats :hover, coming later");
    ui.pointer_button(false);
    assert!(ui.clicked(go) && ui.clicked(document), "clicks count for what the target is in");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(!ui.clicked(go), "a frame forgets clicks");

    ui.pointer_move(500.0, 500.0);
    assert!(!ui.wants_pointer(), "a document's container lets the pointer through");
    ui.pointer_button(true);
    ui.pointer_move(120.0, 310.0);
    ui.pointer_button(false);
    assert!(!ui.clicked(go), "pressed elsewhere, released on it: not a click");

    ui.set_enabled(go, false);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.style(go).unwrap().opacity, 0.4, ":disabled");
    ui.pointer_button(true);
    ui.pointer_button(false);
    assert!(!ui.clicked(go), "disabled elements are not clicked");
}

#[test]
fn classes_and_nearer_sheets_restyle() {
    let (mut ui, document) = cards();
    let plain = ui.find(document, "plain").unwrap();
    ui.add_class(plain, "card");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(background(&ui, plain), Some(Color::hex(0x222222)));

    // Equally specific, but attached nearer: wins.
    ui.add_sheet(plain, StyleSheet::parse("Near.vss", ".card { background-color: #0000ff; }"));
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(background(&ui, plain), Some(Color::rgb(0, 0, 255)));
    let card = ui.find(document, "card").unwrap();
    assert_eq!(background(&ui, card), Some(Color::hex(0x222222)), "a sheet styles only what it is attached to");
}

#[test]
fn mistakes_are_reported_with_their_place() {
    let mut ui = Ui::new();
    let sheet = ".a { colour: red; width: 10em; }\n.b:hovered { color: red; }\n.c { color: #fff; }";
    let markup = "<vui>\n  <Style src=\"Bad.vss\" />\n  <Panel class=\"a\" colour=\"red\" />\n</vui>";
    load(&mut ui, markup, &[("Bad.vss", sheet)]).unwrap();
    ui.frame(WIDTH, HEIGHT, 1.0);
    let warnings = ui.take_warnings();
    let has = |text: &str| warnings.iter().any(|w| w.contains(text));
    assert!(has("Bad.vss:1: unknown property 'colour'"), "{warnings:?}");
    assert!(has("Bad.vss:1: width: expected a length in px or %"), "{warnings:?}");
    assert!(has("Bad.vss:2: unknown state ':hovered'"), "{warnings:?}");
    assert!(has("Test.vui:3:3: <Panel> has no attribute 'colour'"), "{warnings:?}");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(ui.take_warnings().is_empty(), "each is reported once");

    let error = load(&mut ui, "<vui>\n  <Lable>Hi</Lable>\n</vui>", &[]).unwrap_err();
    assert_eq!(
        error.0,
        "Test.vui:2:3: unknown element <Lable>; expected Panel, ScrollView, ListView, TreeView, SplitView, Tabs, Label, Button, TextField, Toggle, Slider, Foldout, Dropdown or Style"
    );
    let error = load(&mut ui, "<vui><Style src=\"Missing.vss\" /></vui>", &[]).unwrap_err();
    assert!(error.0.contains("could not read Missing.vss"), "{error}");
    assert_eq!(ui.children(ui.root()).len(), 1, "a failed document leaves nothing behind");
}
