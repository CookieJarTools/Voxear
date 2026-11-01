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
//!
//! A panel can also float, in a window of its own over the dock: Float on
//! its tab's menu ([`Ui::float_dock_panel`](crate::Ui::float_dock_panel)),
//! or its tab dropped outside every area. Dragging the window's bar moves
//! it, its corner resizes it, and its tabs dock again as any tab does;
//! other tabs dropped on it join it. Floating windows follow the tree in
//! the text form, each with its place and size: `float(600, 80, 420, 320, [inspector])`.

use crate::{
    NodeId, Style, Ui, UiError,
    controls::Control,
    tree::{Element, Kind},
};

/// How far the pointer moves on a pressed tab before it becomes a drag.
const DRAG_START: f32 = 6.0;
/// A floating window's size when it first floats, and the least it can be.
const FLOAT_SIZE: (f32, f32) = (420.0, 320.0);
const FLOAT_MIN: (f32, f32) = (180.0, 120.0);
/// The square in a floating window's corner that resizes it.
const GRIP: f32 = 14.0;
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

/// A floating window of panels over the dock: an area of its own, placed
/// anywhere.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Floating {
    pub id: u32,
    pub panels: Vec<String>,
    pub selected: usize,
    /// Left, top, width and height in logical pixels, from the dock's top left.
    pub rect: [f32; 4],
    /// The window, its tabs, and the grip in its corner, as built.
    pub window: Option<taffy::NodeId>,
    pub tabs: Option<taffy::NodeId>,
    pub grip: Option<taffy::NodeId>,
}

pub(crate) struct DockState {
    pub layout: DockNode,
    pub floating: Vec<Floating>,
    pub panels: Vec<DockPanel>,
    /// Panels closed, each with one it shared an area with, to go back
    /// beside when opened again.
    pub closed: Vec<(String, Option<String>)>,
    /// Each tab button, and the panel it shows.
    pub headers: Vec<(taffy::NodeId, String)>,
    /// Each area's and floating window's options button, at its tab bar's
    /// right end, and the tabs it is for.
    pub menus: Vec<(taffy::NodeId, taffy::NodeId)>,
    /// What was built for the layout, replaced whenever it changes.
    pub structure: Option<taffy::NodeId>,
    pub next_area: u32,
}

/// A press on a tab that may become a drag, or on a floating window's bar
/// or grip, which moves or resizes it.
pub(crate) struct DockDrag {
    pub dock: taffy::NodeId,
    pub panel: String,
    pub start: (f32, f32),
    pub dragging: bool,
    pub ghost: Option<taffy::NodeId>,
    pub preview: Option<taffy::NodeId>,
    pub target: Option<(u32, Zone)>,
    pub window: Option<WindowDrag>,
}

/// A floating window being moved or resized: its area, whether it is
/// resized, and its place when pressed.
pub(crate) type WindowDrag = (u32, bool, [f32; 4]);

impl DockState {
    /// Every panel placed, docked or floating.
    fn placed(&self) -> Vec<String> {
        let floating = self.floating.iter().flat_map(|f| f.panels.iter().cloned());
        self.layout.panels().into_iter().map(str::to_owned).chain(floating).collect()
    }

    /// Takes a panel out wherever it is; a floating window left empty goes.
    fn remove_panel(&mut self, panel: &str) {
        self.layout.remove(panel);
        for floating in &mut self.floating {
            if let Some(index) = floating.panels.iter().position(|p| p == panel) {
                floating.panels.remove(index);
                floating.selected = floating.selected.min(floating.panels.len().saturating_sub(1));
            }
        }
        self.floating.retain(|f| !f.panels.is_empty());
    }

    /// The panels sharing an area or a floating window with `panel`, it included.
    fn area_with(&self, panel: &str) -> Option<Vec<String>> {
        area_with(&self.layout, panel)
            .or_else(|| self.floating.iter().find(|f| f.panels.iter().any(|p| p == panel)).map(|f| f.panels.clone()))
    }
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

/// Reads the text form of a layout: the tree, then any floating windows.
fn parse_layout(text: &str, next_area: &mut u32) -> Result<(DockNode, Vec<Floating>), String> {
    let mut reader = Reader { text: text.as_bytes(), at: 0 };
    let node = reader.node(next_area)?;
    let mut floating = Vec::new();
    loop {
        reader.space();
        if reader.at == reader.text.len() {
            break;
        }
        if reader.word() != "float" {
            return Err(format!("unexpected '{}' after the layout", &text[reader.at..]));
        }
        reader.expect(b'(')?;
        let mut rect = [0.0; 4];
        for (index, value) in rect.iter_mut().enumerate() {
            let word = reader.word();
            *value = word.parse().map_err(|_| format!("float needs its left, top, width and height, not '{word}'"))?;
            if index < 3 {
                reader.expect(b',')?;
            }
        }
        reader.expect(b',')?;
        let DockNode::Area { id, panels, selected, .. } = reader.node(next_area)? else {
            return Err("a floating window holds an area's panels, as [inspector]".to_string());
        };
        reader.expect(b')')?;
        floating.push(Floating { id, panels, selected, rect, window: None, tabs: None, grip: None });
    }
    Ok((node, floating))
}

/// The text form of the floating windows, to follow the tree's.
fn write_floating(floating: &[Floating], out: &mut String) {
    for window in floating {
        let [x, y, w, h] = window.rect.map(f32::round);
        out.push_str(&format!(" float({x}, {y}, {w}, {h}, "));
        DockNode::area(window.id, window.panels.clone(), window.selected).write(out);
        out.push(')');
    }
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
            floating: Vec::new(),
            panels: Vec::new(),
            closed: Vec::new(),
            headers: Vec::new(),
            menus: Vec::new(),
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
        if !state.placed().iter().any(|p| p == id)
            && let DockNode::Area { panels, .. } = state.layout.first_area()
        {
            panels.push(id.to_string());
        }
        self.rebuild_dock(dock);
        content
    }

    /// Adds a panel while the dock is in use, as a tab beside `beside` (or in
    /// the first area, when that is not open), showing: a second Inspector,
    /// say, for a window that can be open more than once. Returns the
    /// element to fill with its contents.
    pub fn add_dock_panel_beside(&mut self, dock: NodeId, id: &str, title: &str, beside: &str) -> NodeId {
        let content = self.add(dock, Element::new(Kind::Panel, Style::column().grow()));
        self.add_class(content, "dock-panel");
        let Some(state) = self.dock_state(dock) else { return content };
        state.panels.push(DockPanel { id: id.to_string(), title: title.to_string(), icon: None, content: content.0 });
        match state.floating.iter_mut().find(|f| f.panels.iter().any(|p| p == beside)) {
            Some(window) => {
                window.panels.push(id.to_string());
                window.selected = window.panels.len() - 1;
            }
            None => {
                let target = area_id_with(&state.layout, beside);
                let target = target.unwrap_or_else(|| match state.layout.first_area() {
                    DockNode::Area { id, .. } => *id,
                    DockNode::Split { .. } => 0,
                });
                state.layout.insert(target, Zone::Center, id, 0.0, 0);
            }
        }
        self.rebuild_dock(dock);
        self.show_dock_panel(dock, id);
        content
    }

    /// Takes a panel out for good, its elements with it, as a window that
    /// can be open more than once does when it closes. An area left empty
    /// folds away.
    pub fn remove_dock_panel(&mut self, dock: NodeId, id: &str) {
        let Some(state) = self.dock_state(dock) else { return };
        let Some(index) = state.panels.iter().position(|p| p.id == id) else { return };
        let content = state.panels.remove(index).content;
        state.remove_panel(id);
        state.closed.retain(|(closed, _)| closed != id);
        state.closed.iter_mut().filter(|(_, n)| n.as_deref() == Some(id)).for_each(|(_, n)| *n = None);
        // Before the rebuild, which would take it out with the tabs it is in.
        self.remove(NodeId(content));
        self.rebuild_dock(dock);
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
        self.arrange_dock(dock, layout, false)
    }

    /// Arranges the panels as `layout` says, closing those it leaves out, as
    /// a workspace that shows only some panels does. They open again in the
    /// first area.
    pub fn set_dock_layout_closing_rest(&mut self, dock: NodeId, layout: &str) -> Result<(), UiError> {
        self.arrange_dock(dock, layout, true)
    }

    fn arrange_dock(&mut self, dock: NodeId, layout: &str, close_rest: bool) -> Result<(), UiError> {
        let Some(state) = self.dock_state(dock) else { return Err(UiError("not a dock space".into())) };
        let mut next = state.next_area;
        let (mut node, floating) = parse_layout(layout, &mut next).map_err(|e| UiError(format!("dock layout: {e}")))?;
        let floats = floating.iter().flat_map(|f| f.panels.iter().cloned());
        let named: Vec<String> = node.panels().iter().map(|p| p.to_string()).chain(floats).collect();
        state.closed.clear();
        for panel in state.panels.iter().filter(|p| !named.contains(&p.id)) {
            if close_rest {
                state.closed.push((panel.id.clone(), None));
            } else if let DockNode::Area { panels, .. } = node.first_area() {
                panels.push(panel.id.clone());
            }
        }
        state.layout = node;
        state.floating = floating;
        state.next_area = next;
        self.rebuild_dock(dock);
        Ok(())
    }

    /// Closes a panel: its tab goes, an area left empty folds away, and its
    /// elements stay alive out of sight until it opens again. Any panel can
    /// close, the last one too, which leaves the dock empty until one opens.
    /// Each area's options button, at its tab bar's right end, is where the
    /// editor offers it, with what else is done to a window.
    pub fn close_dock_panel(&mut self, dock: NodeId, id: &str) {
        let Some(state) = self.dock_state(dock) else { return };
        if !state.placed().iter().any(|p| p == id) {
            return;
        }
        let neighbour = state.area_with(id).and_then(|area| area.into_iter().find(|p| p != id));
        state.remove_panel(id);
        state.closed.push((id.to_string(), neighbour));
        self.rebuild_dock(dock);
    }

    /// Opens a panel, as a tab beside one it shared an area with when it
    /// closed, or else in the first area, and shows it. One already open is
    /// just shown.
    pub fn open_dock_panel(&mut self, dock: NodeId, id: &str) {
        self.open_dock_panel_near(dock, id, None);
    }

    /// Opens a panel as [`open_dock_panel`](Self::open_dock_panel) does,
    /// but beside `beside`, when that is open, rather than in the first area
    /// when it has no neighbour to go back to: a wide panel can open by the
    /// views instead of in a side column.
    pub fn open_dock_panel_beside(&mut self, dock: NodeId, id: &str, beside: &str) {
        self.open_dock_panel_near(dock, id, Some(beside));
    }

    fn open_dock_panel_near(&mut self, dock: NodeId, id: &str, beside: Option<&str>) {
        let Some(state) = self.dock_state(dock) else { return };
        if !state.placed().iter().any(|p| p == id) && state.panels.iter().any(|p| p.id == id) {
            let neighbour = state.closed.iter().find(|(closed, _)| closed == id).and_then(|(_, n)| n.clone());
            state.closed.retain(|(closed, _)| closed != id);
            let target = neighbour.and_then(|n| area_id_with(&state.layout, &n));
            let target = target.or_else(|| beside.and_then(|b| area_id_with(&state.layout, b)));
            let target = target.unwrap_or_else(|| match state.layout.first_area() {
                DockNode::Area { id, .. } => *id,
                DockNode::Split { .. } => 0,
            });
            state.layout.insert(target, Zone::Center, id, 0.0, 0);
            self.rebuild_dock(dock);
        }
        self.show_dock_panel(dock, id);
    }

    /// Every panel, in the order added: its id, title, and whether it is open.
    pub fn dock_panels(&mut self, dock: NodeId) -> Vec<(String, String, bool)> {
        let Some(state) = self.dock_state(dock) else { return Vec::new() };
        let placed = state.placed();
        state.panels.iter().map(|p| (p.id.clone(), p.title.clone(), placed.contains(&p.id))).collect()
    }

    /// The panel whose tab was context-clicked since the last frame, for its
    /// menu, with where the pointer was.
    /// A context click on a tab bar's empty part is one on its tab showing.
    pub fn dock_tab_context_clicked(&mut self, dock: NodeId) -> Option<(String, (f32, f32))> {
        let headers = self.dock_state(dock)?.headers.clone();
        let on_tab = headers.into_iter().find(|(header, _)| self.context_clicked(NodeId(*header)));
        let panel = match on_tab {
            Some((_, panel)) => panel,
            None => self.dock_bar_context_clicked(dock)?,
        };
        Some((panel, self.pointer_position().unwrap_or_default()))
    }

    /// The panel showing in the area or floating window whose tab bar was
    /// context-clicked, away from its tabs.
    fn dock_bar_context_clicked(&mut self, dock: NodeId) -> Option<String> {
        let state = self.dock_state(dock)?;
        let mut areas = Vec::new();
        state.layout.areas(&mut areas);
        let mut tabs: Vec<taffy::NodeId> = areas.into_iter().map(|(_, element, _)| element).collect();
        tabs.extend(state.floating.iter().filter_map(|f| f.tabs));
        let tabs = tabs
            .into_iter()
            .map(NodeId)
            .find(|&tabs| self.children(tabs).first().is_some_and(|&bar| self.context_clicked(bar)))?;
        self.showing_in(dock, tabs)
    }

    /// The panel whose area's options button was clicked since the last
    /// frame, the one showing there, with where its menu opens: under the
    /// button, to its right edge.
    pub fn dock_menu_clicked(&mut self, dock: NodeId) -> Option<(String, (f32, f32))> {
        let menus = self.dock_state(dock)?.menus.clone();
        let (button, tabs) = menus.into_iter().find(|(button, _)| self.clicked(NodeId(*button)))?;
        let [x, y, width, height] = self.rect(NodeId(button));
        Some((self.showing_in(dock, NodeId(tabs))?, (x + width, y + height + 4.0)))
    }

    /// The panel whose tab is selected among an area's or window's tabs.
    fn showing_in(&mut self, dock: NodeId, tabs: NodeId) -> Option<String> {
        let selected = self.selected_tab(tabs);
        let header = self.children(self.children(tabs)[0]).get(selected).copied()?;
        let headers = &self.dock_state(dock)?.headers;
        headers.iter().find(|(node, _)| *node == header.0).map(|(_, panel)| panel.clone())
    }

    /// The other panels in the area a panel is in.
    pub fn dock_panel_neighbours(&mut self, dock: NodeId, id: &str) -> Vec<String> {
        let Some(state) = self.dock_state(dock) else { return Vec::new() };
        state.area_with(id).unwrap_or_default().into_iter().filter(|p| p != id).collect()
    }

    /// The layout as it is now, with any divider moves and tab choices, in
    /// the form [`set_dock_layout`](Self::set_dock_layout) reads.
    pub fn dock_layout(&mut self, dock: NodeId) -> String {
        self.sync_dock(dock);
        let mut out = String::new();
        if let Some(state) = self.dock_state(dock) {
            state.layout.write(&mut out);
            write_floating(&state.floating, &mut out);
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
        for floating in &mut state.floating {
            if let Some(tabs) = floating.tabs {
                floating.selected = self.selected_tab(NodeId(tabs));
            }
        }
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
        // Every floating window built before, those the state has let go of too (one
        // emptied by docking its last panel, or left out of a layout): none may stay.
        for floating in &mut state.floating {
            floating.window = None;
        }
        for child in self.children(dock) {
            if self.has_class(child, "dock-float") {
                self.remove(child);
            }
        }
        state.headers.clear();
        state.menus.clear();
        let mut layout = std::mem::replace(&mut state.layout, DockNode::area(0, Vec::new(), 0));
        let structure = self.build_dock(dock, &mut layout, &mut state);
        state.layout = layout;
        state.structure = Some(structure.0);
        // Floating windows after the tree, so they are over it and the pointer finds them first.
        let mut floating = std::mem::take(&mut state.floating);
        for window in &mut floating {
            self.build_floating(dock, window, &mut state);
        }
        state.floating = floating;
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
                let tabs = self.build_area(parent, panels, *selected, state);
                *element = Some(tabs.0);
                tabs
            }
        };
        let mut style = self.element(built).map(|e| e.base.clone()).unwrap_or_default();
        style.layout.flex_grow = 1.0;
        style.layout.flex_basis = taffy::Dimension::length(0.0);
        self.set_style(built, style);
        built
    }

    /// An area's tabs, a page for each of its panels, with the panels' own
    /// elements moved in.
    fn build_area(&mut self, parent: NodeId, panels: &[String], selected: usize, state: &mut DockState) -> NodeId {
        let tabs = self.tabs(parent);
        self.add_class(tabs, "dock-area");
        for id in panels {
            let Some(panel) = state.panels.iter().find(|p| &p.id == id) else { continue };
            let page = self.add_tab_with_icon(tabs, &panel.title, panel.icon.as_deref());
            let _ = self.tree.add_child(page.0, panel.content);
            self.set_visible(NodeId(panel.content), true);
            let bar = self.children(tabs)[0];
            let header = *self.children(bar).last().expect("add_tab added a tab");
            state.headers.push((header.0, id.clone()));
        }
        // After the tabs, so they keep their places among the bar's children.
        let bar = self.children(tabs)[0];
        let options = self.icon_button(bar, "more");
        self.add_class(options, "dock-menu");
        state.menus.push((options.0, tabs.0));
        self.set_selected_tab(tabs, selected);
        tabs
    }

    /// A floating window: an area's tabs in a card of its own over the dock,
    /// with a grip in its corner.
    fn build_floating(&mut self, dock: NodeId, floating: &mut Floating, state: &mut DockState) {
        let [x, y, w, h] = floating.rect;
        let window = self.panel(dock, absolute(x, y, Some((w, h))));
        self.add_class(window, "dock-float");
        let tabs = self.build_area(window, &floating.panels, floating.selected, state);
        let mut style = self.element(tabs).map(|e| e.base.clone()).unwrap_or_default();
        style.layout.flex_grow = 1.0;
        self.set_style(tabs, style);
        let mut corner = Style::default();
        corner.layout.position = taffy::Position::Absolute;
        corner.layout.inset.right = taffy::LengthPercentageAuto::length(0.0);
        corner.layout.inset.bottom = taffy::LengthPercentageAuto::length(0.0);
        corner.layout.size =
            taffy::Size { width: taffy::Dimension::length(GRIP), height: taffy::Dimension::length(GRIP) };
        let grip = self.panel(window, corner);
        self.add_class(grip, "dock-float-grip");
        (floating.window, floating.tabs, floating.grip) = (Some(window.0), Some(tabs.0), Some(grip.0));
    }

    /// Floats a panel in a window of its own over the dock, at `at` (where
    /// the pointer let go of its tab, in logical pixels) or else in the
    /// dock's middle. The last panel docked stays docked.
    pub fn float_dock_panel(&mut self, dock: NodeId, id: &str, at: Option<(f32, f32)>) {
        self.sync_dock(dock);
        let [dx, dy, dw, dh] = self.rect(dock);
        let Some(state) = self.dock_state(dock) else { return };
        let docked = state.layout.panels();
        if !state.panels.iter().any(|p| p.id == id) || docked == [id] {
            return;
        }
        let (w, h) = FLOAT_SIZE;
        let (x, y) = match at {
            Some((px, py)) => (px - dx - 40.0, py - dy - 12.0),
            None => ((dw - w) / 2.0, (dh - h) / 3.0),
        };
        state.remove_panel(id);
        state.closed.retain(|(closed, _)| closed != id);
        state.layout.forget_elements();
        state.next_area += 1;
        let rect = [x.clamp(0.0, (dw - w).max(0.0)), y.clamp(0.0, (dh - h).max(0.0)), w, h];
        let floating = Floating {
            id: state.next_area,
            panels: vec![id.to_string()],
            selected: 0,
            rect,
            window: None,
            tabs: None,
            grip: None,
        };
        state.floating.push(floating);
        self.rebuild_dock(dock);
        self.mark_changed(dock);
    }

    /// Whether a panel is in a floating window.
    pub fn dock_panel_floating(&mut self, dock: NodeId, id: &str) -> bool {
        self.dock_state(dock).is_some_and(|state| state.floating.iter().any(|f| f.panels.iter().any(|p| p == id)))
    }

    /// Docks a floating panel again, as a tab in the first area.
    pub fn dock_floating_panel(&mut self, dock: NodeId, id: &str) {
        self.sync_dock(dock);
        let Some(state) = self.dock_state(dock) else { return };
        if !state.floating.iter().any(|f| f.panels.iter().any(|p| p == id)) {
            return;
        }
        state.remove_panel(id);
        let target = match state.layout.first_area() {
            DockNode::Area { id, .. } => *id,
            DockNode::Split { .. } => 0,
        };
        state.layout.insert(target, Zone::Center, id, 0.0, 0);
        state.layout.forget_elements();
        self.rebuild_dock(dock);
        self.mark_changed(dock);
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
        let drag = |dock, panel, window| DockDrag {
            dock,
            panel,
            start: point,
            dragging: false,
            ghost: None,
            preview: None,
            target: None,
            window,
        };
        self.dock_drag = match found {
            Some((dock, panel)) => Some(drag(dock, panel, None)),
            None => self.window_pressed(path).map(|(dock, window)| drag(dock, String::new(), Some(window))),
        };
    }

    /// The floating window whose bar or grip `path` presses, if any: its
    /// dock, area, whether the grip was pressed, and its place.
    fn window_pressed(&mut self, path: &[taffy::NodeId]) -> Option<(taffy::NodeId, WindowDrag)> {
        let dock = path.iter().copied().find(|&node| {
            matches!(self.tree.get_node_context(node).and_then(|e| e.control.as_ref()), Some(Control::Dock(_)))
        })?;
        let windows = self.dock_state(NodeId(dock))?.floating.clone();
        let pressed = path.last().copied();
        let found = windows.into_iter().find_map(|window| {
            let bar = window.tabs.and_then(|tabs| self.tree.children(tabs).ok()?.first().copied());
            if pressed.is_some() && pressed == window.grip {
                return Some((window.id, true, window.rect));
            }
            (pressed.is_some() && pressed == bar).then_some((window.id, false, window.rect))
        });
        found.map(|window| (dock, window))
    }

    /// A floating window's bar or grip dragged: it moves, or resizes, with
    /// the pointer, kept inside the dock.
    fn drag_window(&mut self, dock: NodeId, (area, resize, from): WindowDrag, delta: (f32, f32)) {
        let [_, _, dock_w, dock_h] = self.rect(dock);
        let Some(state) = self.dock_state(dock) else { return };
        let Some(floating) = state.floating.iter_mut().find(|f| f.id == area) else { return };
        let [x, y, w, h] = from;
        floating.rect = if resize {
            let (w, h) = ((w + delta.0).max(FLOAT_MIN.0), (h + delta.1).max(FLOAT_MIN.1));
            [x, y, w.min((dock_w - x).max(FLOAT_MIN.0)), h.min((dock_h - y).max(FLOAT_MIN.1))]
        } else {
            [(x + delta.0).clamp(0.0, (dock_w - w).max(0.0)), (y + delta.1).clamp(0.0, (dock_h - h).max(0.0)), w, h]
        };
        let ([x, y, w, h], window) = (floating.rect, floating.window);
        if let Some(window) = window {
            let mut style = self.element(NodeId(window)).map(|e| e.base.clone()).unwrap_or_default();
            style.layout.inset.left = taffy::LengthPercentageAuto::length(x);
            style.layout.inset.top = taffy::LengthPercentageAuto::length(y);
            style.layout.size = taffy::Size { width: taffy::Dimension::length(w), height: taffy::Dimension::length(h) };
            self.set_style(NodeId(window), style);
        }
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
        if let Some(window) = drag.window {
            let delta = (point.0 - drag.start.0, point.1 - drag.start.1);
            if let Some(drag) = self.dock_drag.as_mut() {
                drag.dragging = true;
            }
            self.pointer_cancel();
            return self.drag_window(dock, window, delta);
        }
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
        let target = self.dock_target(dock, &panel, point);
        if let Some(preview) = preview {
            // Nowhere to dock, it floats where it is let go: its window is shown there.
            let (w, h) = FLOAT_SIZE;
            let rect = target.map_or([point.0 - 40.0, point.1 - 12.0, w, h], |(_, rect)| rect);
            self.set_style(preview, absolute(rect[0], rect[1], Some((rect[2], rect[3]))));
            self.set_visible(preview, true);
        }
        if let Some(drag) = self.dock_drag.as_mut() {
            drag.target = target.map(|(target, _)| target);
        }
    }

    /// The area and zone under `point`, and the rectangle the panel would
    /// fill: over a tab bar, the area or floating window it heads, as a tab;
    /// near an area's edge, a split. The middle of an area or window is no
    /// place to dock, so a panel let go there floats, unless it is the one
    /// it came from, where it stays.
    fn dock_target(&mut self, dock: NodeId, panel: &str, point: (f32, f32)) -> Option<((u32, Zone), [f32; 4])> {
        let inside = |[x, y, w, h]: [f32; 4]| (x..x + w).contains(&point.0) && (y..y + h).contains(&point.1);
        let state = self.dock_state(dock)?;
        let windows: Vec<(u32, taffy::NodeId, taffy::NodeId, bool)> = state
            .floating
            .iter()
            .rev()
            .filter_map(|f| Some((f.id, f.window?, f.tabs?, f.panels.iter().any(|p| p == panel))))
            .collect();
        for (area, window, tabs, own) in windows {
            let rect = self.rect(NodeId(window));
            if inside(rect) {
                let on_bar = self.children(NodeId(tabs)).first().is_some_and(|&bar| inside(self.rect(bar)));
                return (on_bar || own).then_some(((area, Zone::Center), rect));
            }
        }
        let mut areas = Vec::new();
        self.dock_state(dock)?.layout.areas(&mut areas);
        let own = self.dock_state(dock).and_then(|state| area_id_with(&state.layout, panel));
        areas.into_iter().find_map(|(id, element, _)| {
            let [x, y, w, h] = self.rect(NodeId(element));
            if !inside([x, y, w, h]) {
                return None;
            }
            let on_bar = self.children(NodeId(element)).first().is_some_and(|&bar| inside(self.rect(bar)));
            let (u, v) = ((point.0 - x) / w, (point.1 - y) / h);
            let (zone, rect) = if on_bar {
                (Zone::Center, [x, y, w, h])
            } else if u < EDGE {
                (Zone::Left, [x, y, w / 2.0, h])
            } else if u > 1.0 - EDGE {
                (Zone::Right, [x + w / 2.0, y, w / 2.0, h])
            } else if v < EDGE {
                (Zone::Top, [x, y, w, h / 2.0])
            } else if v > 1.0 - EDGE {
                (Zone::Bottom, [x, y + h / 2.0, w, h / 2.0])
            } else if own == Some(id) {
                // Back in the middle of where it came from, it stays.
                (Zone::Center, [x, y, w, h])
            } else {
                return None;
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
        if drag.window.is_some() {
            // Moved or resized as it went; the layout already has it.
            self.mark_changed(dock);
            return true;
        }
        let Some((area, zone)) = drag.target else {
            // Let go outside every area: it floats there.
            if let Some(point) = self.pointer_position() {
                self.float_dock_panel(dock, &drag.panel, Some(point));
            }
            return true;
        };
        self.sync_dock(dock);
        let size = self.dock_area_size(dock, area, zone);
        if let Some(state) = self.dock_state(dock) {
            let in_window = state.floating.iter().position(|f| f.id == area);
            // Dropped back on its own window, it stays as it was.
            let alone = match in_window {
                Some(index) => state.floating[index].panels.contains(&drag.panel),
                None => matches!(find_area(&state.layout, area), Some(panels) if panels == [drag.panel.clone()]),
            };
            let last_docked = state.layout.panels() == [drag.panel.as_str()];
            if !alone && !(in_window.is_some() && last_docked) {
                state.remove_panel(&drag.panel);
                match state.floating.iter_mut().find(|f| f.id == area) {
                    Some(window) => {
                        window.panels.push(drag.panel.clone());
                        window.selected = window.panels.len() - 1;
                    }
                    None => {
                        state.next_area += 1;
                        let new_area = state.next_area;
                        state.layout.insert(area, zone, &drag.panel, size, new_area);
                    }
                }
                // What was built no longer matches, so nothing is read back from it.
                state.layout.forget_elements();
            }
        }
        self.rebuild_dock(dock);
        self.mark_changed(dock);
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

/// The panels of the area a panel is in.
fn area_with(node: &DockNode, panel: &str) -> Option<Vec<String>> {
    match node {
        DockNode::Area { panels, .. } => panels.iter().any(|p| p == panel).then(|| panels.clone()),
        DockNode::Split { first, second, .. } => area_with(first, panel).or_else(|| area_with(second, panel)),
    }
}

/// The id of the area a panel is in.
fn area_id_with(node: &DockNode, panel: &str) -> Option<u32> {
    match node {
        DockNode::Area { id, panels, .. } => panels.iter().any(|p| p == panel).then_some(*id),
        DockNode::Split { first, second, .. } => area_id_with(first, panel).or_else(|| area_id_with(second, panel)),
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
        let (layout, floating) = parse_layout(text, &mut next).unwrap();
        let mut out = String::new();
        layout.write(&mut out);
        assert_eq!(out, text);
        assert_eq!(next, 3, "three areas");
        assert!(floating.is_empty());

        // Floating windows follow the tree, each with its place and size.
        let floats = "row(240, [a], [b]) float(600, 80, 420, 320, [c, d*]) float(10, 20, 200, 150, [e])";
        let (layout, floating) = parse_layout(floats, &mut next).unwrap();
        let mut out = String::new();
        layout.write(&mut out);
        write_floating(&floating, &mut out);
        assert_eq!(out, floats);
        assert_eq!((floating[0].rect, floating[0].selected), ([600.0, 80.0, 420.0, 320.0], 1));
        assert!(parse_layout("[a] float(1, 2, [b])", &mut next).unwrap_err().contains("left, top, width and height"));
        assert!(parse_layout("row(240, [a])", &mut next).unwrap_err().contains("expected ','"));
        assert!(parse_layout("grid(1, [a], [b])", &mut next).unwrap_err().contains("expected row, column or ["));
    }

    #[test]
    fn removing_folds_empty_areas_and_inserting_splits() {
        let mut next = 0;
        let (mut layout, _) = parse_layout("row(200, [a], column(300, [b], [c]))", &mut next).unwrap();
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
