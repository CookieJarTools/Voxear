//! Docking: the editor's layout. A dock space is a tree of split panes
//! whose leaves are tabbed areas of panels. Dragging a panel's tab onto
//! another area's middle adds it there as a tab; onto an edge, it splits that
//! area. Empty areas fold away. A panel keeps its own elements wherever it
//! goes, so nothing in it is rebuilt or loses its state.
//!
//! The layout reads and writes as text, for workspace presets and saving:
//!
//! ```text
//! row(240, [hierarchy, project], column(420, [scene, game*], [console]))
//! ```
//!
//! `row` and `column` split side by side or one above the other, with the
//! first part's size in logical pixels, or the second's as a negative
//! number, or the first's share of the whole as a percentage (`57%`), which
//! it keeps as the window resizes; `[...]` is an area's panels, by id, with
//! `*` on the one showing when it is not the first.

use crate::{
    NodeId, Style, Ui, UiError,
    controls::Control,
    tree::{Element, Kind},
};

/// How far the pointer moves on a pressed tab before it becomes a drag.
const DRAG_START: f32 = 6.0;
/// The share of an area along each edge that splits it, rather than joining it.
const EDGE: f32 = 0.25;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DockNode {
    Split {
        row: bool,
        size: f32,
        /// `size` is the first part's share, from 0 to 1, not pixels.
        fraction: bool,
        first: Box<DockNode>,
        second: Box<DockNode>,
        /// The split view built for it, to read its size back from.
        element: Option<taffy::NodeId>,
    },
    Area {
        id: u32,
        panels: Vec<String>,
        selected: usize,
        /// The tabs built for it.
        element: Option<taffy::NodeId>,
    },
}

/// Where a dragged panel would land on an area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

pub(crate) struct DockPanel {
    pub id: String,
    pub title: String,
    /// Shown before the title on its tab.
    pub icon: Option<String>,
    pub content: taffy::NodeId,
}

pub(crate) struct DockState {
    pub layout: DockNode,
    pub panels: Vec<DockPanel>,
    /// Each tab button, and the panel it shows.
    pub headers: Vec<(taffy::NodeId, String)>,
    /// What was built for the layout, replaced whenever it changes.
    pub structure: Option<taffy::NodeId>,
    pub next_area: u32,
}

/// A press on a tab that may become a drag.
pub(crate) struct DockDrag {
    pub dock: taffy::NodeId,
    pub panel: String,
    pub start: (f32, f32),
    pub dragging: bool,
    pub ghost: Option<taffy::NodeId>,
    pub preview: Option<taffy::NodeId>,
    pub target: Option<(u32, Zone)>,
}

impl DockNode {
    fn area(id: u32, panels: Vec<String>, selected: usize) -> Self {
        DockNode::Area { id, panels, selected, element: None }
    }

    fn panels(&self) -> Vec<&str> {
        match self {
            DockNode::Split { first, second, .. } => {
                let mut all = first.panels();
                all.extend(second.panels());
                all
            }
            DockNode::Area { panels, .. } => panels.iter().map(String::as_str).collect(),
        }
    }

    fn first_area(&mut self) -> &mut DockNode {
        match self {
            DockNode::Split { first, .. } => first.first_area(),
            area => area,
        }
    }

    /// Takes `panel` out, folding away an area left empty.
    fn remove(&mut self, panel: &str) {
        match self {
            DockNode::Area { panels, selected, .. } => {
                if let Some(index) = panels.iter().position(|p| p == panel) {
                    panels.remove(index);
                    if *selected >= index && *selected > 0 {
                        *selected -= 1;
                    }
                }
            }
            DockNode::Split { first, second, .. } => {
                first.remove(panel);
                second.remove(panel);
                let first_empty = matches!(**first, DockNode::Area { ref panels, .. } if panels.is_empty());
                let second_empty = matches!(**second, DockNode::Area { ref panels, .. } if panels.is_empty());
                if first_empty {
                    *self = std::mem::replace(second, DockNode::area(0, Vec::new(), 0));
                } else if second_empty {
                    *self = std::mem::replace(first, DockNode::area(0, Vec::new(), 0));
                }
            }
        }
    }

    /// Puts `panel` into area `target`: as its selected tab in the middle,
    /// or in a new area split off one edge, `size` pixels across.
    fn insert(&mut self, target: u32, zone: Zone, panel: &str, size: f32, new_area: u32) -> bool {
        match self {
            DockNode::Area { id, panels, selected, .. } if *id == target => {
                if zone == Zone::Center {
                    panels.push(panel.to_string());
                    *selected = panels.len() - 1;
                    return true;
                }
                let old = std::mem::replace(self, DockNode::area(0, Vec::new(), 0));
                let new = DockNode::area(new_area, vec![panel.to_string()], 0);
                let (first, second) = match zone {
                    Zone::Left | Zone::Top => (new, old),
                    _ => (old, new),
                };
                let row = matches!(zone, Zone::Left | Zone::Right);
                let (first, second) = (Box::new(first), Box::new(second));
                *self = DockNode::Split { row, size, fraction: false, first, second, element: None };
                true
            }
            DockNode::Split { first, second, .. } => {
                first.insert(target, zone, panel, size, new_area) || second.insert(target, zone, panel, size, new_area)
            }
            DockNode::Area { .. } => false,
        }
    }

    fn forget_elements(&mut self) {
        match self {
            DockNode::Split { first, second, element, .. } => {
                *element = None;
                first.forget_elements();
                second.forget_elements();
            }
            DockNode::Area { element, .. } => *element = None,
        }
    }

    fn areas(&self, out: &mut Vec<(u32, taffy::NodeId, usize)>) {
        match self {
            DockNode::Split { first, second, .. } => {
                first.areas(out);
                second.areas(out);
            }
            DockNode::Area { id, element: Some(element), panels, .. } => out.push((*id, *element, panels.len())),
            DockNode::Area { .. } => {}
        }
    }

    /// The text form; see the module documentation.
    fn write(&self, out: &mut String) {
        match self {
            DockNode::Split { row, size, fraction, first, second, .. } => {
                out.push_str(if *row { "row(" } else { "column(" });
                if *fraction {
                    out.push_str(&format!("{}%, ", (size * 100.0).round()));
                } else {
                    out.push_str(&format!("{}, ", size.round()));
                }
                first.write(out);
                out.push_str(", ");
                second.write(out);
                out.push(')');
            }
            DockNode::Area { panels, selected, .. } => {
                out.push('[');
                for (index, panel) in panels.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(panel);
                    if index == *selected && index > 0 {
                        out.push('*');
                    }
                }
                out.push(']');
            }
        }
    }
}

/// Reads the text form of a layout.
fn parse_layout(text: &str, next_area: &mut u32) -> Result<DockNode, String> {
    let mut reader = Reader { text: text.as_bytes(), at: 0 };
    let node = reader.node(next_area)?;
    reader.space();
    if reader.at != reader.text.len() {
        return Err(format!("unexpected '{}' after the layout", &text[reader.at..]));
    }
    Ok(node)
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        self.space();
        if self.text.get(self.at) == Some(&byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at character {}", byte as char, self.at + 1))
        }
    }

    fn word(&mut self) -> String {
        self.space();
        let start = self.at;
        while self.text.get(self.at).is_some_and(|b| b.is_ascii_alphanumeric() || b"-_.".contains(b)) {
            self.at += 1;
        }
        String::from_utf8_lossy(&self.text[start..self.at]).into_owned()
    }

    fn node(&mut self, next_area: &mut u32) -> Result<DockNode, String> {
        self.space();
        if self.text.get(self.at) == Some(&b'[') {
            self.at += 1;
            let (mut panels, mut selected) = (Vec::new(), 0);
            loop {
                self.space();
                if self.text.get(self.at) == Some(&b']') {
                    self.at += 1;
                    break;
                }
                if !panels.is_empty() {
                    self.expect(b',')?;
                }
                let id = self.word();
                if id.is_empty() {
                    return Err(format!("expected a panel id at character {}", self.at + 1));
                }
                self.space();
                if self.text.get(self.at) == Some(&b'*') {
                    self.at += 1;
                    selected = panels.len();
                }
                panels.push(id);
            }
            *next_area += 1;
            return Ok(DockNode::area(*next_area, panels, selected));
        }
        let kind = self.word();
        let row = match kind.as_str() {
            "row" => true,
            "column" => false,
            other => return Err(format!("expected row, column or [, not '{other}'")),
        };
        self.expect(b'(')?;
        let size = self.word().parse::<f32>().map_err(|_| format!("{kind} needs a size first"))?;
        let fraction = self.text.get(self.at) == Some(&b'%');
        if fraction {
            self.at += 1;
        }
        let size = if fraction { (size / 100.0).clamp(0.0, 1.0) } else { size };
        self.expect(b',')?;
        let first = Box::new(self.node(next_area)?);
        self.expect(b',')?;
        let second = Box::new(self.node(next_area)?);
        self.expect(b')')?;
        Ok(DockNode::Split { row, size, fraction, first, second, element: None })
    }
}

impl Ui {
    /// A dock space: areas of tabbed panels in split panes. Give it panels
    /// with [`add_dock_panel`](Self::add_dock_panel) and arrange them with
    /// [`set_dock_layout`](Self::set_dock_layout).
    pub fn dock_space(&mut self, parent: NodeId) -> NodeId {
        let mut style = Style::column().grow();
        style.layout.align_items = taffy::AlignItems::STRETCH;
        let dock = self.add(parent, Element::new(Kind::DockSpace, style));
        let state = DockState {
            layout: DockNode::area(1, Vec::new(), 0),
            panels: Vec::new(),
            headers: Vec::new(),
            structure: None,
            next_area: 1,
        };
        if let Some(element) = self.element_mut(dock) {
            element.control = Some(Control::Dock(Box::new(state)));
        }
        self.rebuild_dock(dock);
        dock
    }

    /// Adds a panel, and returns the element to fill with its contents. It
    /// goes where the layout names its `id`, or else into the first area.
    pub fn add_dock_panel(&mut self, dock: NodeId, id: &str, title: &str) -> NodeId {
        let content = self.add(dock, Element::new(Kind::Panel, Style::column().grow()));
        self.add_class(content, "dock-panel");
        let Some(state) = self.dock_state(dock) else { return content };
        state.panels.push(DockPanel { id: id.to_string(), title: title.to_string(), icon: None, content: content.0 });
        if !state.layout.panels().contains(&id)
            && let DockNode::Area { panels, .. } = state.layout.first_area()
        {
            panels.push(id.to_string());
        }
        self.rebuild_dock(dock);
        content
    }

    /// Shows an icon before a panel's title on its tab.
    pub fn set_dock_panel_icon(&mut self, dock: NodeId, id: &str, icon: &str) {
        let Some(state) = self.dock_state(dock) else { return };
        if let Some(panel) = state.panels.iter_mut().find(|p| p.id == id) {
            panel.icon = Some(icon.to_string());
        }
        self.rebuild_dock(dock);
    }

    /// Arranges the panels as `layout` says; see the [module](self)
    /// documentation for its form. Panels it leaves out join the first area.
    pub fn set_dock_layout(&mut self, dock: NodeId, layout: &str) -> Result<(), UiError> {
        let Some(state) = self.dock_state(dock) else { return Err(UiError("not a dock space".into())) };
        let mut next = state.next_area;
        let mut node = parse_layout(layout, &mut next).map_err(|e| UiError(format!("dock layout: {e}")))?;
        let named: Vec<String> = node.panels().iter().map(|p| p.to_string()).collect();
        for panel in state.panels.iter().filter(|p| !named.contains(&p.id)) {
            if let DockNode::Area { panels, .. } = node.first_area() {
                panels.push(panel.id.clone());
            }
        }
        state.layout = node;
        state.next_area = next;
        self.rebuild_dock(dock);
        Ok(())
    }

    /// The layout as it is now, with any divider moves and tab choices, in
    /// the form [`set_dock_layout`](Self::set_dock_layout) reads.
    pub fn dock_layout(&mut self, dock: NodeId) -> String {
        self.sync_dock(dock);
        let mut out = String::new();
        if let Some(state) = self.dock_state(dock) {
            state.layout.write(&mut out);
        }
        out
    }

    /// Shows a panel, selecting its tab.
    pub fn show_dock_panel(&mut self, dock: NodeId, id: &str) {
        let header = self.dock_state(dock).and_then(|s| s.headers.iter().find(|(_, p)| p == id).map(|(h, _)| *h));
        if let Some(header) = header {
            self.click_panes(header);
        }
    }

    fn dock_state(&mut self, dock: NodeId) -> Option<&mut DockState> {
        match self.element_mut(dock)?.control.as_mut()? {
            Control::Dock(state) => Some(state),
            _ => None,
        }
    }

    /// Reads divider positions and selected tabs back into the layout.
    fn sync_dock(&mut self, dock: NodeId) {
        let Some(mut state) = self.take_dock(dock) else { return };
        self.sync_node(&mut state.layout);
        self.put_dock(dock, state);
    }

    fn sync_node(&self, node: &mut DockNode) {
        match node {
            DockNode::Split { size, fraction, first, second, element, .. } => {
                if let Some(element) = element {
                    let split = NodeId(*element);
                    *size =
                        if *fraction { self.split_fraction(split).unwrap_or(*size) } else { self.split_size(split) };
                }
                self.sync_node(first);
                self.sync_node(second);
            }
            DockNode::Area { selected, element: Some(element), .. } => *selected = self.selected_tab(NodeId(*element)),
            DockNode::Area { .. } => {}
        }
    }

    fn take_dock(&mut self, dock: NodeId) -> Option<Box<DockState>> {
        let element = self.element_mut(dock)?;
        match element.control.take() {
            Some(Control::Dock(state)) => Some(state),
            other => {
                element.control = other;
                None
            }
        }
    }

    fn put_dock(&mut self, dock: NodeId, state: Box<DockState>) {
        if let Some(element) = self.element_mut(dock) {
            element.control = Some(Control::Dock(state));
        }
    }

    /// Builds the split views and tabs the layout describes, moving each
    /// panel's elements into their new place.
    pub(crate) fn rebuild_dock(&mut self, dock: NodeId) {
        let Some(mut state) = self.take_dock(dock) else { return };
        self.sync_node(&mut state.layout);
        for panel in &state.panels {
            if let Some(parent) = self.tree.parent(panel.content) {
                let _ = self.tree.remove_child(parent, panel.content);
            }
        }
        if let Some(old) = state.structure.take() {
            self.remove(NodeId(old));
        }
        state.headers.clear();
        let mut layout = std::mem::replace(&mut state.layout, DockNode::area(0, Vec::new(), 0));
        let structure = self.build_dock(dock, &mut layout, &mut state);
        state.layout = layout;
        state.structure = Some(structure.0);
        // Panels the layout does not place stay out of sight, still alive.
        for panel in &state.panels {
            if self.tree.parent(panel.content).is_none() {
                let _ = self.tree.add_child(dock.0, panel.content);
                self.set_visible(NodeId(panel.content), false);
            }
        }
        self.put_dock(dock, state);
        self.restyle = true;
    }

    fn build_dock(&mut self, parent: NodeId, node: &mut DockNode, state: &mut DockState) -> NodeId {
        let built = match node {
            DockNode::Split { row, size, fraction, first, second, element } => {
                let split = if *fraction {
                    self.split_view_fraction(parent, *row, *size)
                } else {
                    self.split_view(parent, *row, *size)
                };
                *element = Some(split.0);
                let (a, b) = self.split_panes(split).expect("a split view has panes");
                self.build_dock(a, first, state);
                self.build_dock(b, second, state);
                split
            }
            DockNode::Area { panels, selected, element, .. } => {
                let tabs = self.tabs(parent);
                self.add_class(tabs, "dock-area");
                *element = Some(tabs.0);
                for id in panels.iter() {
                    let Some(panel) = state.panels.iter().find(|p| &p.id == id) else { continue };
                    let page = self.add_tab_with_icon(tabs, &panel.title, panel.icon.as_deref());
                    let _ = self.tree.add_child(page.0, panel.content);
                    self.set_visible(NodeId(panel.content), true);
                    let bar = self.children(tabs)[0];
                    let header = *self.children(bar).last().expect("add_tab added a tab");
                    state.headers.push((header.0, id.clone()));
                }
                self.set_selected_tab(tabs, *selected);
                tabs
            }
        };
        let mut style = self.element(built).map(|e| e.base.clone()).unwrap_or_default();
        style.layout.flex_grow = 1.0;
        style.layout.flex_basis = taffy::Dimension::length(0.0);
        self.set_style(built, style);
        built
    }

    /// A press on a dock tab, which may become a drag.
    pub(crate) fn press_dock(&mut self, path: &[taffy::NodeId], point: (f32, f32)) {
        let found = path.iter().rev().find_map(|&node| {
            let mut at = Some(node);
            while let Some(current) = at {
                if let Some(Control::Dock(state)) = self.tree.get_node_context(current).and_then(|e| e.control.as_ref())
                {
                    let panel = state.headers.iter().find(|(h, _)| *h == node).map(|(_, p)| p.clone())?;
                    return Some((current, panel));
                }
                at = self.tree.parent(current);
            }
            None
        });
        self.dock_drag = found.map(|(dock, panel)| DockDrag {
            dock,
            panel,
            start: point,
            dragging: false,
            ghost: None,
            preview: None,
            target: None,
        });
    }

    /// The pointer moved with a dock tab pressed: past a few pixels it
    /// becomes a drag, with a ghost of the tab and a preview of where it
    /// would land.
    pub(crate) fn drag_dock(&mut self, point: (f32, f32)) {
        let Some(drag) = self.dock_drag.as_ref() else { return };
        let moved = (point.0 - drag.start.0).hypot(point.1 - drag.start.1);
        if !drag.dragging && moved < DRAG_START {
            return;
        }
        let (dock, panel) = (NodeId(drag.dock), drag.panel.clone());
        if !drag.dragging {
            let title =
                self.dock_state(dock).and_then(|s| s.panels.iter().find(|p| p.id == panel)).map(|p| p.title.clone());
            let layer = self.popup_layer();
            let ghost = self.add(
                layer,
                Element {
                    text: Some(crate::tree::Text::new(&title.unwrap_or_default())),
                    ..Element::new(Kind::Label, absolute(0.0, 0.0, None))
                },
            );
            self.add_class(ghost, "dock-ghost");
            let preview = self.panel(layer, absolute(0.0, 0.0, Some((0.0, 0.0))));
            self.add_class(preview, "dock-preview");
            self.set_visible(preview, false);
            let Some(drag) = self.dock_drag.as_mut() else { return };
            (drag.dragging, drag.ghost, drag.preview) = (true, Some(ghost.0), Some(preview.0));
            // The press became a drag, so it is no click.
            self.pointer_cancel();
        }
        let Some(drag) = self.dock_drag.as_ref() else { return };
        let (ghost, preview) = (drag.ghost.map(NodeId), drag.preview.map(NodeId));
        if let Some(ghost) = ghost {
            self.set_style(ghost, absolute(point.0 + 12.0, point.1 + 8.0, None));
        }
        let target = self.dock_target(dock, point);
        if let Some(preview) = preview {
            match target {
                Some((_, rect)) => {
                    self.set_style(preview, absolute(rect[0], rect[1], Some((rect[2], rect[3]))));
                    self.set_visible(preview, true);
                }
                None => self.set_visible(preview, false),
            }
        }
        if let Some(drag) = self.dock_drag.as_mut() {
            drag.target = target.map(|(target, _)| target);
        }
    }

    /// The area and zone under `point`, and the rectangle the panel would fill.
    fn dock_target(&mut self, dock: NodeId, point: (f32, f32)) -> Option<((u32, Zone), [f32; 4])> {
        let mut areas = Vec::new();
        self.dock_state(dock)?.layout.areas(&mut areas);
        areas.into_iter().find_map(|(id, element, _)| {
            let [x, y, w, h] = self.rect(NodeId(element));
            if !(x..x + w).contains(&point.0) || !(y..y + h).contains(&point.1) {
                return None;
            }
            let (u, v) = ((point.0 - x) / w, (point.1 - y) / h);
            let (zone, rect) = if u < EDGE {
                (Zone::Left, [x, y, w / 2.0, h])
            } else if u > 1.0 - EDGE {
                (Zone::Right, [x + w / 2.0, y, w / 2.0, h])
            } else if v < EDGE {
                (Zone::Top, [x, y, w, h / 2.0])
            } else if v > 1.0 - EDGE {
                (Zone::Bottom, [x, y + h / 2.0, w, h / 2.0])
            } else {
                (Zone::Center, [x, y, w, h])
            };
            Some(((id, zone), rect))
        })
    }

    /// The pointer let go of a dock tab. A drag moves the panel where the
    /// preview showed and is used up; a press that never moved is left to be
    /// the tab's click.
    pub(crate) fn release_dock(&mut self) -> bool {
        let Some(drag) = self.dock_drag.take() else { return false };
        for node in [drag.ghost, drag.preview].into_iter().flatten() {
            self.remove(NodeId(node));
        }
        if !drag.dragging {
            return false;
        }
        let dock = NodeId(drag.dock);
        if let Some((area, zone)) = drag.target {
            self.sync_dock(dock);
            let size = self.dock_area_size(dock, area, zone);
            if let Some(state) = self.dock_state(dock) {
                let alone = matches!(find_area(&state.layout, area), Some(panels) if panels == [drag.panel.clone()]);
                if !alone {
                    state.next_area += 1;
                    let new_area = state.next_area;
                    let mut layout = state.layout.clone();
                    layout.remove(&drag.panel);
                    if layout.insert(area, zone, &drag.panel, size, new_area) {
                        // What was built no longer matches, so nothing is read back from it.
                        layout.forget_elements();
                        state.layout = layout;
                    }
                }
            }
            self.rebuild_dock(dock);
            self.mark_changed(dock);
        }
        true
    }

    /// Half an area's width or height, for a split off its edge.
    fn dock_area_size(&mut self, dock: NodeId, area: u32, zone: Zone) -> f32 {
        let mut areas = Vec::new();
        if let Some(state) = self.dock_state(dock) {
            state.layout.areas(&mut areas);
        }
        let element = areas.iter().find(|(id, ..)| *id == area).map(|(_, e, _)| NodeId(*e));
        let [_, _, w, h] = element.map_or([0.0; 4], |e| self.rect(e));
        if matches!(zone, Zone::Left | Zone::Right) { w / 2.0 } else { h / 2.0 }
    }
}

/// An area's panels, by id.
fn find_area(node: &DockNode, area: u32) -> Option<Vec<String>> {
    match node {
        DockNode::Area { id, panels, .. } if *id == area => Some(panels.clone()),
        DockNode::Area { .. } => None,
        DockNode::Split { first, second, .. } => find_area(first, area).or_else(|| find_area(second, area)),
    }
}

/// Placed at `x`, `y` on the screen, out of the flow, letting the pointer through.
fn absolute(x: f32, y: f32, size: Option<(f32, f32)>) -> Style {
    let mut style = Style { pickable: false, ..Style::default() }.at(x, y);
    if let Some((w, h)) = size {
        style.layout.size = taffy::Size { width: taffy::Dimension::length(w), height: taffy::Dimension::length(h) };
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_read_and_write_the_same() {
        let text = "row(240, [hierarchy, project], column(420, [scene, game*], [console]))";
        let mut next = 0;
        let layout = parse_layout(text, &mut next).unwrap();
        let mut out = String::new();
        layout.write(&mut out);
        assert_eq!(out, text);
        assert_eq!(next, 3, "three areas");
        assert!(parse_layout("row(240, [a])", &mut next).unwrap_err().contains("expected ','"));
        assert!(parse_layout("grid(1, [a], [b])", &mut next).unwrap_err().contains("expected row, column or ["));
    }

    #[test]
    fn removing_folds_empty_areas_and_inserting_splits() {
        let mut next = 0;
        let mut layout = parse_layout("row(200, [a], column(300, [b], [c]))", &mut next).unwrap();
        layout.remove("b");
        let mut out = String::new();
        layout.write(&mut out);
        assert_eq!(out, "row(200, [a], [c])", "b's area went, and its split with it");

        let c = match &layout {
            DockNode::Split { second, .. } => match **second {
                DockNode::Area { id, .. } => id,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        assert!(layout.insert(c, Zone::Bottom, "b", 150.0, 99));
        assert!(layout.insert(c, Zone::Center, "d", 0.0, 100));
        let mut out = String::new();
        layout.write(&mut out);
        assert_eq!(out, "row(200, [a], column(150, [c, d*], [b]))");
    }
}
