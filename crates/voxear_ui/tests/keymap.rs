//! Key maps, without a GPU: the devices fitted into the map, each key where
//! its data says, and the labels round them, apart, each with a line of its
//! own that crosses no key, label or other line.

use voxear_ui::{
    ArtLook, ArtShape, Color, KeyShape, KeyState, KeymapArt, KeymapData, KeymapDevice, KeymapKey, KeymapLine,
    KeymapSide, Shape, Style, Ui,
};

fn key(rect: [f32; 4], label: &str, callout: &[&str]) -> KeymapKey {
    KeymapKey {
        rect,
        shape: KeyShape::Key,
        label: label.into(),
        state: if callout.is_empty() { KeyState::Unused } else { KeyState::Assigned },
        callout: callout.iter().map(|s| KeymapLine::with_chip(s, label)).collect(),
        ..KeymapKey::default()
    }
}

fn map_with(data: KeymapData, size: [u32; 2]) -> (Ui, voxear_ui::NodeId) {
    let mut ui = Ui::new();
    let root = ui.root();
    let holder = ui.panel(root, Style::cover());
    let map = ui.keymap(holder);
    ui.set_keymap(map, data);
    // Laid out, then arranged round the labels' sizes, then the keys' text sized.
    for _ in 0..3 {
        ui.frame(size[0], size[1], 1.0);
    }
    (ui, map)
}

#[test]
fn a_key_map_fits_its_board_and_places_its_labels_round_it() {
    let mut keys = vec![
        key([0.0, 0.0, 1.0, 1.0], "Esc", &["Pause"]),
        key([2.0, 0.0, 1.0, 1.0], "F1", &["Help"]),
        key([0.0, 2.0, 1.5, 1.0], "Tab", &["Inventory", "Swap"]),
        key([8.0, 2.0, 1.0, 1.0], "P", &["Map"]),
        key([3.0, 4.0, 4.0, 1.0], "Space", &["Jump"]),
        key([5.0, 2.0, 1.0, 1.0], "G", &[]),
    ];
    keys[3].color = Some(Color::rgb(0xe0, 0x80, 0x20));
    let (ui, map) = map_with(KeymapData { size: [10.0, 5.0], keys, ..KeymapData::default() }, [1000, 600]);

    let rect = |at: usize| ui.keymap_key_rect(map, at).unwrap();
    let (esc, space, tab, p) = (rect(0), rect(4), rect(2), rect(3));
    assert!(space[2] > esc[2] * 3.5, "keys keep their widths: {esc:?} {space:?}");
    let callouts = all(&ui, map, "keymap-callout");
    assert_eq!(callouts.len(), 5, "a label for each bound key");
    let label_of = |text: &str| {
        let at = callouts.iter().position(|&c| all(&ui, c, "keymap-callout-line").iter().any(|&l| ui.text(l) == text));
        ui.rect(callouts[at.unwrap()])
    };
    assert!(label_of("Pause")[1] + label_of("Pause")[3] <= esc[1], "above");
    assert!(label_of("Inventory")[0] + label_of("Inventory")[2] <= tab[0], "to the left");
    assert!(label_of("Map")[0] >= p[0] + p[2], "to the right");
    assert!(label_of("Jump")[1] >= space[1] + space[3], "below");
    let tab_label = callouts.iter().find(|&&c| ui.has_class(c, "left")).copied().unwrap();
    assert_eq!(all(&ui, tab_label, "keymap-callout-row").len(), 2, "a row for each of Tab's actions");
    let chip = all(&ui, tab_label, "keymap-chip")[0];
    let text = all(&ui, tab_label, "keymap-callout-line")[0];
    assert!(ui.rect(chip)[0] > ui.rect(text)[0], "on the left, the chip comes after the name, nearest the line");
    assert_eq!(ui.text(chip), "Tab");
    assert_clean(&ui, map);
    assert_eq!(
        ui.style(nth_key(&ui, map, 3)).unwrap().background,
        Some(Color::rgb(0xe0, 0x80, 0x20)),
        "its own colour"
    );
}

#[test]
fn inner_keys_lead_their_lines_through_the_gap_below_their_row() {
    // A row of five keys with labels and an unlabelled one at each end, over another row, as a
    // keyboard's: no more than three go to one side, and the inner ones drop into the gap to run out.
    let mut keys = vec![key([0.0, 0.0, 1.0, 1.0], "~", &[]), key([7.0, 0.0, 1.0, 1.0], "=", &[])];
    for (at, name) in ["A", "S", "D", "F", "G"].iter().enumerate() {
        keys.push(key([1.0 + at as f32, 0.0, 1.0, 1.0], name, &[name]));
    }
    keys.push(key([0.0, 1.6, 8.0, 1.0], "Bar", &[]));
    keys.push(key([0.0, -1.6, 8.0, 1.0], "Top", &[]));
    keys.push(key([0.0, 3.2, 8.0, 1.0], "Bottom", &[]));
    let devices = vec![KeymapDevice { rect: [-0.2, -1.8, 8.4, 6.2], rows: true }];
    let (ui, map) = map_with(KeymapData { size: [8.0, 4.0], keys, art: Vec::new(), devices }, [1000, 600]);
    let lefts = all(&ui, map, "keymap-callout").into_iter().filter(|&c| ui.has_class(c, "left")).count();
    assert_eq!(lefts, 3, "three of the row to the left, the rest to the right");
    let lines = ui.keymap_lines(map);
    let origin = ui.rect(map);
    let d = ui.keymap_key_rect(map, 4).unwrap();
    let (_, through_d) = lines.iter().find(|(key, _)| *key == 4).unwrap();
    let under = [d[0] + d[2] / 2.0 - origin[0], d[1] + d[3] - origin[1]];
    let close = (through_d[0][0] - under[0]).abs() < 0.6 && (through_d[0][1] - under[1]).abs() < 0.6;
    assert!(close, "D's drops from under it: {through_d:?} {d:?}");
    assert!(through_d[1][1] > through_d[0][1], "into the gap below");
    assert_clean(&ui, map);
}

#[test]
fn a_key_map_draws_its_art_fits_its_text_and_marks_conflicts() {
    let mut keys = vec![
        key([0.0, 0.0, 1.0, 1.0], "W", &["Jump"]),
        key([1.0, 0.0, 1.5, 1.0], "Backspace", &[]),
        key([3.0, 0.0, 2.0, 1.0], "E", &["Use", "Open"]),
        KeymapKey {
            shape: KeyShape::Path { path: "M0 10L10 0L20 10Z", view: [20.0, 10.0] },
            ..key([6.0, 0.0, 2.0, 1.0], "LT", &["Aim"])
        },
    ];
    keys[2].state = KeyState::Conflict;
    keys[0].callout[0].icon = Some("play".into());
    let art = vec![
        KeymapArt {
            rect: [-0.2, -0.2, 8.4, 1.4],
            shape: ArtShape::Rect { radius: 0.3 },
            look: ArtLook::Plate,
            device: 0,
        },
        KeymapArt { rect: [1.0, 0.5, 1.0, 1.0], shape: ArtShape::Circle, look: ArtLook::Recess, device: 0 },
    ];
    let (ui, map) = map_with(KeymapData { size: [12.0, 3.0], keys, art, devices: Vec::new() }, [800, 400]);

    let board = all(&ui, map, "keymap-board")[0];
    let shapes = ui.shapes(board);
    assert!(
        matches!(shapes[0], Shape::Rect { .. }) && matches!(shapes[1], Shape::Rect { .. }),
        "the plate's edge and fill"
    );
    assert!(matches!(shapes[2], Shape::Circle { .. }), "the well");
    let trigger = nth_key(&ui, map, 3);
    assert!(ui.has_class(trigger, "shaped"));
    assert!(ui.shapes(trigger).iter().any(|s| matches!(s, Shape::Path { .. })), "a shaped key is drawn as its path");

    let (w, back) = (nth_key(&ui, map, 0), nth_key(&ui, map, 1));
    let label = |key| ui.children(key)[0];
    let text = |key| ui.text_style(label(key)).unwrap().size;
    assert!(text(back) < text(w), "a long name is made smaller: {} {}", text(back), text(w));
    assert!(ui.rect(label(back))[2] <= ui.rect(back)[2], "and fits inside its key");

    assert!(ui.has_class(nth_key(&ui, map, 2), "conflict"));
    let callouts = all(&ui, map, "keymap-callout");
    let conflicts: Vec<_> = callouts.iter().copied().filter(|&c| ui.has_class(c, "conflict")).collect();
    assert_eq!(conflicts.len(), 1, "its label marked too");
    assert_eq!(all(&ui, conflicts[0], "keymap-callout-line").len(), 2, "both actions");
    assert_eq!(all(&ui, conflicts[0], "keymap-chip").len(), 1, "and one chip");
    assert_eq!(all(&ui, conflicts[0], "keymap-callout-warn").len(), 1, "with a warning");
    assert_eq!(all(&ui, map, "keymap-callout-icon").len(), 1, "an action's icon beside its name");
    assert_clean(&ui, map);
}

#[test]
fn a_key_asking_for_a_side_has_its_label_there() {
    let keys = vec![KeymapKey { side: Some(KeymapSide::Top), ..key([4.0, 2.0, 1.0, 1.0], "K", &["Kick"]) }];
    let (ui, map) = map_with(KeymapData { size: [8.0, 4.0], keys, ..KeymapData::default() }, [800, 400]);
    let callout = all(&ui, map, "keymap-callout")[0];
    let k = ui.keymap_key_rect(map, 0).unwrap();
    assert!(ui.has_class(callout, "top"));
    assert!(ui.rect(callout)[1] + ui.rect(callout)[3] <= k[1], "above");
    let (_, line) = &ui.keymap_lines(map)[0];
    assert!(line.windows(2).all(|p| p[0][0] == p[1][0]), "straight up: {line:?}");
}

/// Every line runs level or upright, from its key's edge to its label's,
/// and comes near no other key, label or line.
fn assert_clean(ui: &Ui, map: voxear_ui::NodeId) {
    assert_eq!(ui.keymap_lines(map).len(), all(ui, map, "keymap-callout").len(), "a line for each label");
    let problems = ui.keymap_crossings(map);
    assert!(problems.is_empty(), "{problems:#?}");
}

fn nth_key(ui: &Ui, map: voxear_ui::NodeId, at: usize) -> voxear_ui::NodeId {
    all(ui, map, "keymap-key")[at]
}

fn all(ui: &Ui, node: voxear_ui::NodeId, class: &str) -> Vec<voxear_ui::NodeId> {
    let mut out = Vec::new();
    collect(ui, node, class, &mut out);
    out
}

fn collect(ui: &Ui, node: voxear_ui::NodeId, class: &str, out: &mut Vec<voxear_ui::NodeId>) {
    if ui.has_class(node, class) {
        out.push(node);
    }
    for child in ui.children(node) {
        collect(ui, child, class, out);
    }
}
