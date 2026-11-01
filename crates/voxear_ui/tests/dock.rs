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

/// The middle of an area's tab bar, past its tabs, where a tab dropped joins it.
fn bar_of(ui: &Ui, area: NodeId) -> (f32, f32) {
    let [x, y, w, h] = ui.rect(ui.children(area)[0]);
    (x + w * 0.75, y + h / 2.0)
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
    let target = bar_of(&ui, scene_area);
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
fn a_tab_let_go_in_the_middle_of_another_area_floats() {
    let (mut ui, dock) = editor();
    let inspector = tab(&ui, dock, "Inspector");
    let scene_area = ui.parent(ui.parent(tab(&ui, dock, "Scene")).unwrap()).unwrap();
    let (from, middle) = (centre(&ui, inspector), centre(&ui, scene_area));
    drag(&mut ui, from, middle);
    assert!(
        ui.dock_panel_floating(dock, "inspector"),
        "out on its own, not a tab of the Scene's: {}",
        ui.dock_layout(dock)
    );
    assert!(ui.dock_layout(dock).starts_with("row(240, [hierarchy, project], [scene]) float("));
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

#[test]
fn panels_close_and_open_again_beside_where_they_were() {
    let (mut ui, dock) = editor();
    ui.close_dock_panel(dock, "project");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.dock_layout(dock), "row(240, [hierarchy], column(380, [scene], [inspector]))");
    let open: Vec<(String, bool)> = ui.dock_panels(dock).into_iter().map(|(id, _, open)| (id, open)).collect();
    assert!(open.contains(&("project".into(), false)), "listed as closed: {open:?}");

    ui.open_dock_panel(dock, "project");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(
        ui.dock_layout(dock),
        "row(240, [hierarchy, project*], column(380, [scene], [inspector]))",
        "back beside the Hierarchy, showing"
    );

    // A tab's close button closes it too.
    let close = with_class(&ui, tab(&ui, dock, "Project"), "dock-tab-close").expect("a close button on the tab");
    let (x, y) = centre(&ui, close);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_button(false);
    assert_eq!(ui.dock_tab_close_clicked(dock).as_deref(), Some("project"));
    ui.close_dock_panel(dock, "project");
    ui.frame(WIDTH, HEIGHT, 1.0);
    ui.open_dock_panel(dock, "project");
    ui.frame(WIDTH, HEIGHT, 1.0);

    // A whole area closes, folding away; its panel comes back to the first area.
    ui.close_dock_panel(dock, "inspector");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.dock_layout(dock), "row(240, [hierarchy, project*], [scene])");
    ui.open_dock_panel(dock, "inspector");
    assert!(ui.dock_layout(dock).contains("inspector"));
    ui.frame(WIDTH, HEIGHT, 1.0);

    let (x, y) = centre(&ui, tab(&ui, dock, "Scene"));
    ui.pointer_move(x, y);
    ui.pointer_secondary(true);
    ui.pointer_secondary(false);
    assert_eq!(ui.dock_tab_context_clicked(dock).map(|(panel, _)| panel), Some("scene".to_string()), "a tab's menu");
    ui.frame(WIDTH, HEIGHT, 1.0);
    let hierarchy_area = ui.parent(ui.parent(tab(&ui, dock, "Hierarchy")).unwrap()).unwrap();
    let (x, y) = bar_of(&ui, hierarchy_area);
    ui.pointer_move(x, y);
    ui.pointer_secondary(true);
    ui.pointer_secondary(false);
    let menu = ui.dock_tab_context_clicked(dock).map(|(panel, _)| panel);
    // The Inspector opened again into that area a moment ago, so it is the tab showing there.
    assert_eq!(menu.as_deref(), Some("inspector"), "the bar's empty part is its tab's showing");
    ui.frame(WIDTH, HEIGHT, 1.0);

    // A layout that leaves panels out can close them rather than gather them in the first area.
    ui.set_dock_layout_closing_rest(dock, "row(240, [hierarchy], [scene])").unwrap();
    ui.frame(WIDTH, HEIGHT, 1.0);
    let open: Vec<String> = ui.dock_panels(dock).into_iter().filter(|(.., open)| *open).map(|(id, ..)| id).collect();
    assert_eq!(open, ["hierarchy", "scene"]);
    ui.open_dock_panel(dock, "inspector");
    assert!(ui.dock_layout(dock).contains("inspector"), "and they open again");
}

/// The first element with `class` in `node`.
fn with_class(ui: &Ui, node: NodeId, class: &str) -> Option<NodeId> {
    if ui.has_class(node, class) {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| with_class(ui, child, class))
}

/// A floating window's place and size, as the layout writes it.
fn floated(layout: &str) -> Vec<f32> {
    let inside = layout.split("float(").nth(1).expect("a floating window");
    inside.split(',').take(4).map(|n| n.trim().parse().unwrap()).collect()
}

#[test]
fn panels_float_in_windows_that_move_resize_and_dock_again() {
    let (mut ui, dock) = editor();
    let name = ui.find(dock, "name").unwrap();
    ui.set_text(name, "Lamp (edited)");
    ui.float_dock_panel(dock, "inspector", None);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let layout = ui.dock_layout(dock);
    assert!(layout.starts_with("row(240, [hierarchy, project], [scene]) float("), "{layout}");
    assert!(layout.ends_with("[inspector])"), "{layout}");
    assert!(ui.dock_panel_floating(dock, "inspector"));
    assert_eq!((ui.find(dock, "name"), ui.text(name).as_str()), (Some(name), "Lamp (edited)"), "moved, not rebuilt");

    // Its bar moves it; its corner resizes it.
    let window = with_class(&ui, dock, "dock-float").unwrap();
    let bar = with_class(&ui, window, "tab-bar").unwrap();
    let [bx, by, bw, bh] = ui.rect(bar);
    let before = floated(&layout);
    drag(&mut ui, (bx + bw - 20.0, by + bh / 2.0), (bx + bw + 30.0, by + bh / 2.0 + 20.0));
    let moved = floated(&ui.dock_layout(dock));
    assert_eq!((moved[0] - before[0], moved[1] - before[1]), (50.0, 20.0), "{moved:?} from {before:?}");
    let grip = with_class(&ui, window, "dock-float-grip").unwrap();
    let from = centre(&ui, grip);
    drag(&mut ui, from, (from.0 + 40.0, from.1 + 30.0));
    let sized = floated(&ui.dock_layout(dock));
    assert_eq!((sized[2] - moved[2], sized[3] - moved[3]), (40.0, 30.0));

    // A tab dropped on it joins it; one of its tabs dropped on an area docks there.
    let window = with_class(&ui, dock, "dock-float").unwrap();
    let into = bar_of(&ui, ui.children(window)[0]);
    let project = centre(&ui, tab(&ui, dock, "Project"));
    drag(&mut ui, project, into);
    assert!(ui.dock_layout(dock).ends_with("[inspector, project*])"), "{}", ui.dock_layout(dock));
    let inspector = centre(&ui, tab(&ui, dock, "Inspector"));
    drag(&mut ui, inspector, (inspector.0 + 10.0, inspector.1 + 40.0));
    assert!(ui.dock_layout(dock).ends_with("[inspector, project*])"), "dropped on its own window, it stays");
    let hierarchy_area = ui.parent(ui.parent(tab(&ui, dock, "Hierarchy")).unwrap()).unwrap();
    let target = bar_of(&ui, hierarchy_area);
    let inspector = centre(&ui, tab(&ui, dock, "Inspector"));
    drag(&mut ui, inspector, target);
    let layout = ui.dock_layout(dock);
    assert!(layout.starts_with("row(240, [hierarchy, inspector*], [scene]) float("), "{layout}");
    assert!(layout.ends_with("[project])"), "{layout}");

    // A tab let go outside every area floats there.
    let hierarchy = centre(&ui, tab(&ui, dock, "Hierarchy"));
    drag(&mut ui, hierarchy, (hierarchy.0, HEIGHT as f32 + 80.0));
    let layout = ui.dock_layout(dock);
    assert_eq!(layout.matches("float(").count(), 2, "{layout}");
    assert!(layout.starts_with("row(240, [inspector], [scene]) float("), "{layout}");

    // The layout reads back the same, and a floating panel docks again from code.
    ui.set_dock_layout(dock, &layout).unwrap();
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.dock_layout(dock), layout);
    ui.dock_floating_panel(dock, "project");
    assert!(!ui.dock_panel_floating(dock, "project"));
    assert!(
        ui.dock_layout(dock).starts_with("row(240, [inspector, project*], [scene]) float("),
        "{}",
        ui.dock_layout(dock)
    );
    let warnings = ui.take_warnings();
    assert!(warnings.is_empty(), "{warnings:?}");
}
