//! Grid layout, transitions, split views, tabs and dropdowns, without a GPU.

use voxear_ui::{Color, Key, MenuItem, NodeId, Quad, Style, StyleSheet, TextStyle, Ui};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

fn load(ui: &mut Ui, markup: &str) -> NodeId {
    let root = ui.root();
    let document = ui.load_markup(root, "Test.vui", markup, &mut |_| Err("no files".into())).unwrap();
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(ui.take_warnings().is_empty());
    document
}

#[test]
fn grids_place_children_in_tracks() {
    let mut ui = Ui::new();
    let document = load(
        &mut ui,
        r#"<vui>
            <Panel name="grid" style="display: grid; grid-template-columns: 100px 1fr; gap: 10px; width: 400px">
                <Label name="a">Name</Label><TextField name="b" />
                <Label name="c">Speed</Label><Slider name="d" />
                <Label name="wide" style="grid-column: 1 / span 2">All the way across</Label>
            </Panel>
        </vui>"#,
    );
    let rect = |name: &str| ui.rect(ui.find(document, name).unwrap());
    assert_eq!((rect("a")[0], rect("b")[0]), (0.0, 110.0), "a 100px column, a gap, then the rest");
    assert_eq!(rect("b")[2], 290.0, "1fr takes what is left of 400");
    assert_eq!(rect("c")[0], 0.0, "the third child starts the second row");
    assert!(rect("c")[1] > rect("a")[1]);
    assert_eq!(rect("wide")[2], 400.0, "spanning both columns");
}

#[test]
fn transitions_ease_between_styles_but_not_on_creation() {
    let mut ui = Ui::bare();
    ui.advance(0.0);
    let root = ui.root();
    let sheet = ".box { background-color: #000000; transition: background-color 100ms linear; } .box.lit { background-color: #ffffff; }";
    ui.add_sheet(root, StyleSheet::parse("Fade.vss", sheet));
    let node = ui.panel(root, Style::default().at(0.0, 0.0).width(10.0).height(10.0));
    ui.add_class(node, "box");
    let red = |ui: &mut Ui| ui.frame(WIDTH, HEIGHT, 1.0).quads.iter().find(|q| q.kind == Quad::SHAPE).unwrap().color[0];
    assert_eq!(red(&mut ui), 0.0, "new elements appear as they are");
    assert!(!ui.is_animating());

    ui.add_class(node, "lit");
    assert_eq!(red(&mut ui), 0.0, "the change starts from what was showing");
    assert!(ui.is_animating());
    ui.advance(0.05);
    let half = red(&mut ui);
    assert!((half - 0.5).abs() < 0.02, "halfway in linear light, got {half}");
    ui.advance(0.06);
    assert_eq!(red(&mut ui), 1.0);
    assert!(!ui.is_animating(), "finished animations are dropped");
    assert_eq!(ui.style(node).unwrap().background, Some(Color::WHITE));

    let mut still = Ui::bare();
    let root = still.root();
    still.add_sheet(root, StyleSheet::parse("Fade.vss", sheet));
    let node = still.panel(root, Style::default().at(0.0, 0.0).width(10.0).height(10.0));
    still.add_class(node, "box");
    still.frame(WIDTH, HEIGHT, 1.0);
    still.add_class(node, "lit");
    assert_eq!(red(&mut still), 1.0, "with no clock, there is no transition to wait for");
}

#[test]
fn keyframes_play_over_and_over_or_a_few_times() {
    let mut ui = Ui::bare();
    ui.advance(0.0);
    let root = ui.root();
    let sheet = r#"
        :root { --lit: #ffffff; }
        @keyframes pulse { from { background-color: #000000; } to { background-color: var(--lit); } }
        @keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }
        .dot { background-color: #000000; animation: pulse 100ms linear infinite alternate; }
        .once { background-color: #ff0000; animation: pulse 100ms linear 50ms 2; }
        .wink { background-color: #ffffff; animation: blink 200ms linear infinite; }
    "#;
    ui.add_sheet(root, StyleSheet::parse("Pulse.vss", sheet));
    assert!(ui.take_warnings().is_empty());
    let node = ui.panel(root, Style::default().at(0.0, 0.0).width(10.0).height(10.0));
    ui.add_class(node, "dot");
    let quad = |ui: &mut Ui| *ui.frame(WIDTH, HEIGHT, 1.0).quads.iter().find(|q| q.kind == Quad::SHAPE).unwrap();
    let red = |ui: &mut Ui| quad(ui).color[0];
    assert_eq!(red(&mut ui), 0.0, "it starts at its first keyframe");
    assert!(ui.is_animating(), "and plays over and over");
    ui.advance(0.05);
    assert!((red(&mut ui) - 0.5).abs() < 0.02, "halfway between them, var() and all");
    ui.advance(0.05);
    assert_eq!(red(&mut ui), 1.0);
    ui.advance(0.05);
    assert!((red(&mut ui) - 0.5).abs() < 0.02, "alternate goes back the other way");
    assert!(ui.take_warnings().is_empty());

    // Twice, after a delay, then its own colour again.
    ui.remove_class(node, "dot");
    ui.add_class(node, "once");
    assert_eq!(quad(&mut ui).color[..3], [1.0, 0.0, 0.0], "its own colour during the delay");
    ui.advance(0.1);
    assert!((red(&mut ui) - 0.5).abs() < 0.02, "then plays");
    ui.advance(0.2);
    assert_eq!(quad(&mut ui).color[..3], [1.0, 0.0, 0.0], "played twice, it shows its own colour");
    assert!(!ui.is_animating());

    // Opacity, with a stop at both ends.
    ui.remove_class(node, "once");
    ui.add_class(node, "wink");
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.advance(0.1);
    assert!(quad(&mut ui).color[3] < 0.02, "faded out at the middle stop");

    // Mistakes say what was wrong.
    let broken = StyleSheet::parse(
        "Broken.vss",
        "@keyframes spin { from { width: 10px; } } @media print {} .a { animation: nope 1s; }",
    );
    assert!(
        broken.warnings().iter().any(|w| w.contains("@media is not something a sheet can hold")),
        "{:?}",
        broken.warnings()
    );
    ui.add_sheet(root, broken);
    ui.add_class(node, "a");
    ui.frame(WIDTH, HEIGHT, 1.0);
    let warnings = ui.take_warnings();
    assert!(warnings.iter().any(|w| w.contains("no @keyframes named 'nope'")), "{warnings:?}");
}

#[test]
fn split_views_drag_their_divider_within_limits() {
    let mut ui = Ui::new();
    let root = ui.root();
    let split = ui.split_view(root, true, 200.0);
    ui.set_style(split, {
        let mut style = Style::row().at(0.0, 0.0).width(600.0).height(300.0);
        style.layout.align_items = taffy::AlignItems::STRETCH;
        style
    });
    ui.add_class(split, "split-row");
    ui.set_split_min(split, 100.0);
    let (first, second) = ui.split_panes(split).unwrap();
    ui.label(first, "Hierarchy", TextStyle::default());
    ui.label(second, "Scene", TextStyle::default());
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.rect(first)[2], 200.0);
    assert_eq!(ui.rect(second)[2], 600.0 - 200.0 - 4.0, "the rest, after the divider");

    let divider = ui.children(split)[1];
    let [x, y, w, h] = ui.rect(divider);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    assert!(ui.pointer_button(true));
    ui.pointer_move(x + w / 2.0 + 150.0, y + h / 2.0);
    ui.pointer_button(false);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.split_size(split), 350.0);
    assert_eq!(ui.rect(first)[2], 350.0);
    assert!(ui.changed(split) || ui.split_size(split) == 350.0);

    ui.set_split(split, 10.0);
    assert_eq!(ui.split_size(split), 100.0, "no smaller than the minimum");
    ui.set_split(split, 590.0);
    assert_eq!(ui.split_size(split), 600.0 - 100.0 - 4.0, "nor so big the second pane goes under its minimum");
}

#[test]
fn tabs_show_one_page_at_a_time() {
    let mut ui = Ui::new();
    let document = load(
        &mut ui,
        r#"<vui>
            <Tabs name="views" selected="1" style="width: 300px; height: 200px">
                <Tab text="Scene"><Label name="scene">Scene view</Label></Tab>
                <Tab text="Game"><Label name="game">Game view</Label></Tab>
            </Tabs>
        </vui>"#,
    );
    let views = ui.find(document, "views").unwrap();
    let page = |ui: &Ui, name: &str| ui.parent(ui.find(document, name).unwrap()).unwrap();
    assert_eq!(ui.selected_tab(views), 1);
    assert!(ui.is_visible(page(&ui, "game")) && !ui.is_visible(page(&ui, "scene")));

    let bar = ui.children(views)[0];
    let scene_tab = ui.children(bar)[0];
    assert!(!ui.is_checked(scene_tab));
    click(&mut ui, scene_tab);
    assert_eq!(ui.selected_tab(views), 0);
    assert!(ui.changed(views) && ui.is_checked(scene_tab));
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(ui.is_visible(page(&ui, "scene")) && !ui.is_visible(page(&ui, "game")));
}

#[test]
fn dropdowns_open_choose_and_close() {
    let mut ui = Ui::new();
    let document = load(
        &mut ui,
        r#"<vui>
            <Panel style="position: absolute; left: 20px; top: 20px; width: 200px; gap: 8px">
                <Dropdown name="shading" value="1">
                    <Option>Lit</Option><Option>Unlit</Option><Option>Wireframe</Option>
                </Dropdown>
                <Button name="other">Other</Button>
            </Panel>
        </vui>"#,
    );
    let shading = ui.find(document, "shading").unwrap();
    let other = ui.find(document, "other").unwrap();
    assert_eq!((ui.selected_option(shading), ui.text(ui.children(shading)[0])), (1, "Unlit".to_string()));

    click(&mut ui, shading);
    assert!(ui.is_dropdown_open(shading));
    ui.frame(WIDTH, HEIGHT, 1.0);
    let layer = *ui.children(ui.root()).last().unwrap();
    let popup = ui.children(layer)[0];
    assert!(ui.rect(popup)[1] > ui.rect(shading)[1], "the list opens below");
    let options = ui.children(popup);
    assert!(ui.is_checked(options[1]), "the current choice is marked");
    assert!(ui.rect(other)[1] < ui.rect(options[2])[1] + 20.0, "and covers the button below it");
    click(&mut ui, options[2]);
    assert!(!ui.is_dropdown_open(shading));
    assert_eq!(ui.selected_option(shading), 2);
    assert!(ui.changed(shading));
    assert!(!ui.clicked(other), "the option took the click, not what was under it");

    click(&mut ui, shading);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let [x, y, ..] = ui.rect(other);
    ui.pointer_move(x + 150.0, y + 200.0);
    assert!(ui.pointer_button(true), "a press elsewhere closes it and is used up");
    ui.pointer_button(false);
    assert!(!ui.is_dropdown_open(shading));

    ui.focus(shading);
    ui.key_down(Key::Up);
    assert_eq!(ui.selected_option(shading), 1, "arrows move the choice");
    ui.key_down(Key::Enter);
    assert!(ui.is_dropdown_open(shading));
    assert!(ui.key_down(Key::Escape));
    assert!(!ui.is_dropdown_open(shading));
}

#[test]
fn segmented_controls_keep_one_segment_on() {
    let mut ui = Ui::new();
    let document = load(
        &mut ui,
        r#"<vui><Segmented name="tools" value="1" style="position: absolute; left: 0px; top: 0px">
            <Segment icon="select" /><Segment icon="move" /><Segment text="Pivot" icon="pivot" />
        </Segmented></vui>"#,
    );
    let tools = ui.find(document, "tools").unwrap();
    let segments = ui.children(tools);
    assert_eq!(ui.segment(tools), 1);
    assert!(ui.is_checked(segments[1]) && !ui.is_checked(segments[0]));
    click(&mut ui, segments[2]);
    assert!(ui.changed(tools), "a click on another segment is a change");
    assert_eq!(ui.segment(tools), 2);
    assert!(ui.is_checked(segments[2]) && !ui.is_checked(segments[1]), "only one is on");
    ui.frame(WIDTH, HEIGHT, 1.0);
    click(&mut ui, segments[2]);
    assert!(!ui.changed(tools), "clicking the one on changes nothing");
}

#[test]
fn clicks_in_quick_succession_are_a_double_click() {
    let mut ui = Ui::new();
    let root = ui.root();
    let button = ui.button(root, "Open");
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.advance(0.0);
    click(&mut ui, button);
    assert!(ui.clicked(button) && !ui.double_clicked(button));
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.advance(0.1);
    click(&mut ui, button);
    assert!(ui.double_clicked(button), "a second click soon after");
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.advance(0.1);
    click(&mut ui, button);
    assert!(!ui.double_clicked(button), "a third starts again");
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.advance(1.0);
    click(&mut ui, button);
    assert!(!ui.double_clicked(button), "nor is one long after");
}

#[test]
fn sliders_show_their_value_to_their_step() {
    let mut ui = Ui::new();
    let root = ui.root();
    let slider = ui.slider(root, 0.0, 10.0, 2.5);
    let shown = |ui: &Ui| ui.text(*ui.children(slider).last().unwrap());
    assert_eq!(shown(&ui), "2.50", "two decimals without a step");
    ui.set_slider_step(slider, 0.5);
    assert_eq!(shown(&ui), "2.5", "as many as the step has");
    ui.set_slider_step(slider, 1.0);
    assert_eq!(shown(&ui), "3", "and whole numbers for a whole step");
}

#[test]
fn a_dividers_handle_lights_its_middle_over_everything() {
    let mut ui = Ui::new();
    let root = ui.root();
    let split = ui.split_view(root, true, 200.0);
    ui.set_style(split, {
        let mut style = Style::row().at(0.0, 0.0).width(600.0).height(300.0);
        style.layout.align_items = taffy::AlignItems::STRETCH;
        style
    });
    // The second pane is drawn after the divider, and would cover a handle drawn in place.
    let (_, second) = ui.split_panes(split).unwrap();
    ui.set_inline_style(second, "background-color: #333333; margin-left: -8px; pointer-events: none");
    ui.frame(WIDTH, HEIGHT, 1.0);
    let divider = ui.children(split)[1];
    let handle = ui.children(divider)[0];
    let lit = |ui: &Ui| ui.style(handle).unwrap().background.is_some_and(|c| c.a > 0);
    assert!(!lit(&ui), "unlit until the pointer finds it");

    let [x, y, w, h] = ui.rect(divider);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(lit(&ui), "lit while hovered");
    let [hx, hy, hw, hh] = ui.rect(handle);
    assert!(hh < h * 0.5 && hy > y && hy + hh < y + h, "across the middle, short of the ends: {hy} {hh} of {h}");
    assert_eq!(hx + hw / 2.0, x + w / 2.0, "centred on the divider");
    let last = frame.quads.last().unwrap();
    assert_eq!(last.rect[1], hy, "drawn after everything else, the pane over the divider included");

    ui.pointer_button(true);
    ui.pointer_move(x + 300.0, y + h + 50.0);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(lit(&ui), "still lit while dragged, wherever the pointer goes");
    ui.pointer_button(false);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(!lit(&ui), "and unlit once let go");
}

#[test]
fn menus_open_at_the_pointer_choose_and_stay_on_screen() {
    let mut ui = Ui::new();
    let root = ui.root();
    let owner = ui.panel(root, Style::default().at(0.0, 0.0).width(WIDTH as f32).height(HEIGHT as f32));
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.pointer_move(100.0, 120.0);
    ui.pointer_secondary(true);
    ui.pointer_secondary(false);
    assert!(ui.context_clicked(owner));
    let at = ui.pointer_position().unwrap();
    let items = [
        MenuItem::new("Keep"),
        MenuItem::separator(),
        MenuItem::new("Reset").checked(true),
        MenuItem::new("Paste").enabled(false),
    ];
    ui.open_menu(owner, at, &items);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(ui.is_menu_open(owner));
    let layer = *ui.children(ui.root()).last().unwrap();
    let menu = ui.children(layer)[0];
    assert_eq!(ui.rect(menu)[..2], [100.0, 120.0], "it opens at the pointer");
    let rows = ui.children(menu);
    assert_eq!(rows.len(), 4);
    assert!(ui.has_class(rows[1], "menu-separator"));
    assert!(ui.is_checked(rows[2]));

    click(&mut ui, rows[3]);
    assert_eq!(ui.menu_choice(owner), None, "a disabled item cannot be chosen");
    assert!(ui.is_menu_open(owner));
    click(&mut ui, rows[2]);
    assert_eq!(ui.menu_choice(owner), Some(2));
    assert!(!ui.is_menu_open(owner), "a choice closes it");
    assert!(!ui.clicked(owner), "the item took the click, not what was under it");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.menu_choice(owner), None, "choices last a frame");

    // Opened at the bottom right corner, it moves back inside the screen.
    ui.open_menu(owner, (WIDTH as f32 - 10.0, HEIGHT as f32 - 10.0), &items);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let layer = *ui.children(ui.root()).last().unwrap();
    let [x, y, w, h] = ui.rect(ui.children(layer)[0]);
    assert!(x + w <= WIDTH as f32 && y + h <= HEIGHT as f32, "on screen: {x}, {y}, {w} by {h}");
    ui.key_down(Key::Escape);
    assert!(!ui.is_menu_open(owner), "Escape closes it");
}

#[test]
fn a_dropdowns_list_fits_its_options_and_the_screen() {
    let mut ui = Ui::new();
    let document = load(
        &mut ui,
        r#"<vui>
            <Panel style="position: absolute; left: 20px; top: 560px; width: 60px">
                <Dropdown name="narrow">
                    <Option>Lit</Option><Option>A much longer option than the field</Option>
                </Dropdown>
            </Panel>
        </vui>"#,
    );
    let narrow = ui.find(document, "narrow").unwrap();
    click(&mut ui, narrow);
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let layer = *ui.children(ui.root()).last().unwrap();
    let popup = ui.children(layer)[0];
    let ([_, field_top, field_width, _], [_, top, width, height]) = (ui.rect(narrow), ui.rect(popup));
    assert!(width > field_width + 40.0, "as wide as its longest option: {width} for a field of {field_width}");
    assert!(top + height <= field_top, "above the field, since there is no room below: {top} + {height}");

    ui.close_popup();
    ui.set_dropdown_popup_width(narrow, Some(90.0));
    click(&mut ui, narrow);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let layer = *ui.children(ui.root()).last().unwrap();
    assert_eq!(ui.rect(ui.children(layer)[0])[2], 90.0, "unless its width is set");
}
