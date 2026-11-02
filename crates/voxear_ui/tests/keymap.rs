//! Key maps, without a GPU: the board fitted into the middle, each key
//! where its data says, and the labels round the board, apart, with lines
//! out to them.

use voxear_ui::{Color, KeyShape, KeyState, KeymapData, KeymapKey, Shape, Style, Ui};

fn key(rect: [f32; 4], label: &str, callout: &[&str]) -> KeymapKey {
    KeymapKey {
        rect,
        shape: KeyShape::Key,
        label: label.into(),
        state: if callout.is_empty() { KeyState::Unused } else { KeyState::Assigned },
        color: None,
        callout: callout.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn a_key_map_fits_its_board_and_places_its_labels_round_it() {
    let mut ui = Ui::new();
    let root = ui.root();
    let holder = ui.panel(root, Style::cover());
    let map = ui.keymap(holder);
    let mut keys = vec![
        key([0.0, 0.0, 1.0, 1.0], "Esc", &["Pause"]),
        key([2.0, 0.0, 1.0, 1.0], "F1", &["Help"]),
        key([0.0, 2.0, 1.5, 1.0], "Tab", &["Inventory", "Swap"]),
        key([8.0, 2.0, 1.0, 1.0], "P", &["Map"]),
        key([3.0, 4.0, 4.0, 1.0], "Space", &["Jump"]),
        key([5.0, 2.0, 1.0, 1.0], "G", &[]),
    ];
    keys[3].color = Some(Color::rgb(0xe0, 0x80, 0x20));
    ui.set_keymap(map, KeymapData { size: [10.0, 5.0], keys });
    ui.frame(1000, 600, 1.0);

    let rect = |at: usize| ui.keymap_key_rect(map, at).unwrap();
    let (esc, space, tab, p) = (rect(0), rect(4), rect(2), rect(3));
    assert!(space[2] > esc[2] * 3.5, "keys keep their widths: {esc:?} {space:?}");
    let mut callouts = Vec::new();
    collect(&ui, map, "keymap-callout", &mut callouts);
    assert_eq!(callouts.len(), 5, "a label for each bound key");
    let boxes: Vec<[f32; 4]> = callouts.iter().map(|&c| ui.rect(c)).collect();
    // Esc's label is above its key, Tab's to its left, P's to its right, and Space's below.
    let label_of = |text: &str| {
        let at = callouts.iter().position(|&c| {
            let mut lines = Vec::new();
            collect(&ui, c, "keymap-callout-line", &mut lines);
            lines.iter().any(|&l| ui.text(l) == text)
        });
        boxes[at.unwrap()]
    };
    assert!(label_of("Pause")[1] + label_of("Pause")[3] <= esc[1], "above");
    assert!(label_of("Inventory")[0] + label_of("Inventory")[2] <= tab[0], "to the left");
    assert!(label_of("Map")[0] >= p[0] + p[2], "to the right");
    assert!(label_of("Jump")[1] >= space[1] + space[3], "below");
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            let apart = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
            assert!(apart, "labels apart: {a:?} {b:?}");
        }
    }
    let mut lines = Vec::new();
    collect(&ui, map, "keymap-lines", &mut lines);
    let drawn = ui.shapes(lines[0]).iter().filter(|s| matches!(s, Shape::Line { .. })).count();
    assert_eq!(drawn, 5 * 3, "three pieces a line");
    assert_eq!(
        ui.style(nth_key(&ui, map, 3)).unwrap().background,
        Some(Color::rgb(0xe0, 0x80, 0x20)),
        "its own colour"
    );
}

fn nth_key(ui: &Ui, map: voxear_ui::NodeId, at: usize) -> voxear_ui::NodeId {
    let mut keys = Vec::new();
    collect(ui, map, "keymap-key", &mut keys);
    keys[at]
}

fn collect(ui: &Ui, node: voxear_ui::NodeId, class: &str, out: &mut Vec<voxear_ui::NodeId>) {
    if ui.has_class(node, class) {
        out.push(node);
    }
    for child in ui.children(node) {
        collect(ui, child, class, out);
    }
}
