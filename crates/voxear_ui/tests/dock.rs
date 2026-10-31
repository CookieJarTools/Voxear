//! Docking, driven through the pointer as a user would, without a GPU.

use voxear_ui::{NodeId, Ui};

const WIDTH: u32 = 1000;
const HEIGHT: u32 = 600;

const EDITOR: &str = r#"
<vui>
    <DockSpace name="dock" layout="row(240, [hierarchy, project], column(380, [scene], [inspector]))"
               style="position: absolute; left: 0px; top: 0px; width: 1000px; height: 600px">
        <DockPanel id="hierarchy" title="Hierarchy"><Label>Main Camera</Label></DockPanel>
        <DockPanel id="project" title="Project"><Label>Assets</Label></DockPanel>
        <DockPanel id="scene" title="Scene"><Label>Viewport</Label></DockPanel>
        <DockPanel id="inspector" title="Inspector"><TextField name="name" value="Lamp" /></DockPanel>
    </DockSpace>
</vui>"#;

fn editor() -> (Ui, NodeId) {
    let mut ui = Ui::new();
    let root = ui.root();
    let document = ui.load_markup(root, "Editor.vui", EDITOR, &mut |_| Err("no files".into())).unwrap();
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(ui.take_warnings().is_empty());
    let dock = ui.find(document, "dock").unwrap();
    (ui, dock)
}

/// The tab button showing `title`.
fn tab(ui: &Ui, dock: NodeId, title: &str) -> NodeId {
    fn search(ui: &Ui, node: NodeId, title: &str) -> Option<NodeId> {
        if ui.has_class(node, "tab") && ui.text(node) == title {
            return Some(node);
        }
        ui.children(node).into_iter().find_map(|child| search(ui, child, title))
    }
    search(ui, dock, title).unwrap_or_else(|| panic!("no tab {title}"))
}

fn centre(ui: &Ui, node: NodeId) -> (f32, f32) {
    let [x, y, w, h] = ui.rect(node);
    (x + w / 2.0, y + h / 2.0)
}

/// Presses `from`, moves in steps to `to`, and lets go.
fn drag(ui: &mut Ui, from: (f32, f32), to: (f32, f32)) {
    ui.pointer_move(from.0, from.1);
    ui.pointer_button(true);
    for step in 1..=4 {
        let t = step as f32 / 4.0;
        ui.pointer_move(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
    }
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.pointer_button(false);
    ui.frame(WIDTH, HEIGHT, 1.0);
}

#[test]
fn markup_builds_the_layout_it_names() {
    let (mut ui, dock) = editor();
    assert_eq!(ui.dock_layout(dock), "row(240, [hierarchy, project], column(380, [scene], [inspector]))");
    let hierarchy = tab(&ui, dock, "Hierarchy");
    let scene = tab(&ui, dock, "Scene");
    assert!(ui.rect(scene)[0] > ui.rect(hierarchy)[0] + 200.0, "the scene's area is right of the first split");
}

#[test]
fn dragging_a_tab_into_an_area_joins_it_and_keeps_the_panel_alive() {
    let (mut ui, dock) = editor();
    let name = ui.find(dock, "name").unwrap();
    ui.set_text(name, "Lamp (edited)");
    let inspector = tab(&ui, dock, "Inspector");
    let scene_area = ui.parent(ui.parent(tab(&ui, dock, "Scene")).unwrap()).unwrap();
    let target = centre(&ui, scene_area);
    let from = centre(&ui, inspector);
    drag(&mut ui, from, target);

    assert_eq!(
        ui.dock_layout(dock),
        "row(240, [hierarchy, project], [scene, inspector*])",
        "the inspector's area folded away"
    );
    assert_eq!(ui.find(dock, "name"), Some(name), "the same field, moved rather than rebuilt");
    assert_eq!(ui.text(name), "Lamp (edited)");
}

#[test]
fn dragging_a_tab_to_an_edge_splits_the_area() {
    let (mut ui, dock) = editor();
    let project = tab(&ui, dock, "Project");
    let scene_area = ui.parent(ui.parent(tab(&ui, dock, "Scene")).unwrap()).unwrap();
    let [x, y, w, h] = ui.rect(scene_area);
    let from = centre(&ui, project);
    drag(&mut ui, from, (x + w - 10.0, y + h / 2.0));
    let layout = ui.dock_layout(dock);
    assert!(layout.starts_with("row(240, [hierarchy], column(380, row("), "{layout}");
    assert!(layout.contains("[scene], [project])"), "project split off the scene's right edge: {layout}");
}

#[test]
fn a_tab_pressed_without_moving_is_a_click() {
    let (mut ui, dock) = editor();
    let project = tab(&ui, dock, "Project");
    let (x, y) = centre(&ui, project);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_move(x + 2.0, y);
    ui.pointer_button(false);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.dock_layout(dock), "row(240, [hierarchy, project*], column(380, [scene], [inspector]))");
}

#[test]
fn dividers_move_and_the_layout_remembers() {
    let (mut ui, dock) = editor();
    let hierarchy_area = ui.parent(ui.parent(tab(&ui, dock, "Hierarchy")).unwrap()).unwrap();
    let [x, y, w, h] = ui.rect(hierarchy_area);
    let divider = (x + w + 2.0, y + h / 2.0);
    drag(&mut ui, divider, (divider.0 + 60.0, divider.1));
    assert!(ui.dock_layout(dock).starts_with("row(300, "), "{}", ui.dock_layout(dock));

    let layout = ui.dock_layout(dock);
    let (mut other, other_dock) = editor();
    other.set_dock_layout(other_dock, &layout).unwrap();
    other.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(other.dock_layout(other_dock), layout, "a saved layout restores");
    assert!(other.set_dock_layout(other_dock, "row(10, [a]").is_err());
}

#[test]
fn shares_and_right_hand_panels_keep_their_size_as_the_window_resizes() {
    let (mut ui, dock) = editor();
    let layout = "row(-300, column(40%, [hierarchy], [project]), [scene, inspector])";
    ui.set_dock_layout(dock, layout).unwrap();
    ui.frame(WIDTH, HEIGHT, 1.0);
    let area = |ui: &Ui, title: &str| ui.rect(ui.parent(ui.parent(tab(ui, dock, title)).unwrap()).unwrap());
    assert_eq!(area(&ui, "Scene")[2], 300.0, "a negative size is the second part's");
    assert_eq!(area(&ui, "Hierarchy")[3], 240.0, "a percentage is the first part's share");

    ui.set_inline_style(dock, "position: absolute; left: 0px; top: 0px; width: 800px; height: 500px");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(area(&ui, "Scene")[2], 300.0, "the right-hand part keeps its size");
    assert_eq!(area(&ui, "Hierarchy")[3], 200.0, "and the share stays a share");
    assert_eq!(ui.dock_layout(dock), layout, "both write back as they were read");
}
