//! Controls that arrange other UI: split views, tabs and dropdowns, which
//! the editor's docking, panels and toolbars are made from.
//!
//! - **SplitView**: two panes either side of a divider (`split-divider`)
//!   that the pointer drags. The first pane keeps its size and the second
//!   takes the rest, as in Unity's TwoPaneSplitView.
//! - **Tabs**: a `tab-bar` of `tab` buttons over `tab-page`s, showing the
//!   selected one. The selected tab matches `:checked`.
//! - **Dropdown**: the chosen option with an arrow; it opens a
//!   `dropdown-popup` of `dropdown-option`s in a layer above everything else,
//!   which closes on a choice, Escape or a press elsewhere.

use taffy::{AlignItems, Dimension, LengthPercentageAuto, Position};

use crate::{
    NodeId, Overflow, Style, Ui,
    controls::Control,
    input::Key,
    tree::{Element, Kind, Text},
};

/// What a split view keeps.
pub(crate) struct Split {
    pub first: taffy::NodeId,
    pub second: taffy::NodeId,
    /// Side by side, rather than one above the other.
    pub row: bool,
    /// The first pane's size, and the least either pane may have.
    pub size: f32,
    pub min: f32,
    /// Where on the divider it was grabbed.
    pub grab: f32,
}

pub(crate) struct Tabs {
    pub bar: taffy::NodeId,
    pub pages: taffy::NodeId,
    /// Each tab's button and page.
    pub tabs: Vec<(taffy::NodeId, taffy::NodeId)>,
    pub selected: usize,
}

pub(crate) struct Dropdown {
    pub label: taffy::NodeId,
    pub options: Vec<String>,
    pub selected: usize,
    pub popup: Option<taffy::NodeId>,
}

impl Ui {
    /// Two panes side by side (`row`) or one above the other, the first
    /// `size` logical pixels across. Fill them through [`split_panes`](Self::split_panes).
    pub fn split_view(&mut self, parent: NodeId, row: bool, size: f32) -> NodeId {
        let mut style = if row { Style::row() } else { Style::column() };
        style.layout.align_items = AlignItems::STRETCH;
        let split = self.add(parent, Element::new(Kind::SplitView, style));
        self.add_class(split, if row { "split-row" } else { "split-column" });
        let first = self.panel(split, Style { overflow: Overflow::Hidden, ..Style::column() });
        let mut divider = Style::default();
        divider.layout.flex_shrink = 0.0;
        let divider = self.panel(split, divider);
        self.add_class(divider, "split-divider");
        let mut second = Style { overflow: Overflow::Hidden, ..Style::column().grow() };
        second.layout.flex_basis = Dimension::length(0.0);
        let second = self.panel(split, second);
        if let Some(element) = self.element_mut(divider) {
            element.control = Some(Control::Divider(split.0));
        }
        let state = Split { first: first.0, second: second.0, row, size, min: 40.0, grab: 0.0 };
        if let Some(element) = self.element_mut(split) {
            element.control = Some(Control::Split(state));
        }
        self.set_split(split, size);
        split
    }

    /// A split view's two panes, first then second.
    pub fn split_panes(&self, split: NodeId) -> Option<(NodeId, NodeId)> {
        match self.element(split)?.control.as_ref()? {
            Control::Split(state) => Some((NodeId(state.first), NodeId(state.second))),
            _ => None,
        }
    }

    pub fn split_size(&self, split: NodeId) -> f32 {
        match self.element(split).and_then(|e| e.control.as_ref()) {
            Some(Control::Split(state)) => state.size,
            _ => 0.0,
        }
    }

    /// Sets the first pane's size, keeping both panes at least their minimum
    /// as far as the last layout allows.
    pub fn set_split(&mut self, split: NodeId, size: f32) {
        let [_, _, width, height] = self.content_box(split);
        let Some(Control::Split(state)) = self.element_mut(split).and_then(|e| e.control.as_mut()) else { return };
        let total = if state.row { width } else { height };
        let room = total - state.min - 4.0;
        state.size = if room > state.min { size.clamp(state.min, room) } else { size.max(state.min) };
        let (first, row, size) = (NodeId(state.first), state.row, state.size);
        let mut style = self.element(first).map(|e| e.base.clone()).unwrap_or_default();
        style.layout.flex_basis = Dimension::length(size);
        style.layout.flex_shrink = 0.0;
        style.layout.flex_grow = 0.0;
        if row {
            style.layout.min_size.width = LengthPercentageAuto::length(0.0);
        } else {
            style.layout.min_size.height = LengthPercentageAuto::length(0.0);
        }
        self.set_style(first, style);
    }

    /// The least size either pane can be dragged to.
    pub fn set_split_min(&mut self, split: NodeId, min: f32) {
        if let Some(Control::Split(state)) = self.element_mut(split).and_then(|e| e.control.as_mut()) {
            state.min = min.max(0.0);
        }
    }

    /// Tabs over pages, the first selected. Add tabs with [`add_tab`](Self::add_tab).
    pub fn tabs(&mut self, parent: NodeId) -> NodeId {
        let mut style = Style::column();
        style.layout.align_items = AlignItems::STRETCH;
        let tabs = self.add(parent, Element::new(Kind::Tabs, style));
        let bar = self.panel(tabs, Style::row());
        self.add_class(bar, "tab-bar");
        let pages = self.panel(tabs, Style::column().grow());
        self.add_class(pages, "tab-pages");
        let state = Tabs { bar: bar.0, pages: pages.0, tabs: Vec::new(), selected: 0 };
        if let Some(element) = self.element_mut(tabs) {
            element.control = Some(Control::Tabs(state));
        }
        tabs
    }

    /// Adds a tab titled `title`, and returns its page, for what it shows.
    pub fn add_tab(&mut self, tabs: NodeId, title: &str) -> NodeId {
        let Some(Control::Tabs(state)) = self.element(tabs).and_then(|e| e.control.as_ref()) else { return tabs };
        let (bar, pages, index) = (NodeId(state.bar), NodeId(state.pages), state.tabs.len());
        let header = Element {
            text: Some(Text::new(title)),
            control: Some(Control::Tab { tabs: tabs.0, index }),
            ..Element::new(Kind::Button, Style::default())
        };
        let header = self.add(bar, header);
        self.add_class(header, "tab");
        let page = self.panel(pages, Style::column().grow());
        self.add_class(page, "tab-page");
        if let Some(Control::Tabs(state)) = self.element_mut(tabs).and_then(|e| e.control.as_mut()) {
            state.tabs.push((header.0, page.0));
        }
        let selected = self.selected_tab(tabs);
        self.set_selected_tab(tabs, selected);
        page
    }

    pub fn selected_tab(&self, tabs: NodeId) -> usize {
        match self.element(tabs).and_then(|e| e.control.as_ref()) {
            Some(Control::Tabs(state)) => state.selected,
            _ => 0,
        }
    }

    /// Shows tab `index`'s page and hides the others.
    pub fn set_selected_tab(&mut self, tabs: NodeId, index: usize) {
        let Some(Control::Tabs(state)) = self.element_mut(tabs).and_then(|e| e.control.as_mut()) else { return };
        state.selected = index.min(state.tabs.len().saturating_sub(1));
        let (selected, all) = (state.selected, state.tabs.clone());
        for (i, (header, page)) in all.into_iter().enumerate() {
            self.set_checked(NodeId(header), i == selected);
            self.set_visible(NodeId(page), i == selected);
        }
    }

    /// A choice of `options`, showing the one selected.
    pub fn dropdown(&mut self, parent: NodeId, options: &[&str], selected: usize) -> NodeId {
        let dropdown = self.add(parent, Element::new(Kind::Dropdown, Style::row()));
        let label = self
            .add(dropdown, Element { text: Some(Text::new("")), ..Element::new(Kind::Label, Style::default().grow()) });
        self.add_class(label, "dropdown-label");
        let arrow =
            self.add(dropdown, Element { text: Some(Text::new("▾")), ..Element::new(Kind::Label, Style::default()) });
        self.add_class(arrow, "dropdown-arrow");
        let state = Dropdown { label: label.0, options: Vec::new(), selected: 0, popup: None };
        if let Some(element) = self.element_mut(dropdown) {
            element.control = Some(Control::Dropdown(state));
        }
        self.set_options(dropdown, options);
        self.set_selected_option(dropdown, selected);
        dropdown
    }

    pub fn set_options(&mut self, dropdown: NodeId, options: &[&str]) {
        let Some(Control::Dropdown(state)) = self.element_mut(dropdown).and_then(|e| e.control.as_mut()) else {
            return;
        };
        state.options = options.iter().map(|o| o.to_string()).collect();
        let selected = state.selected;
        self.close_popup();
        self.set_selected_option(dropdown, selected);
    }

    pub fn selected_option(&self, dropdown: NodeId) -> usize {
        match self.element(dropdown).and_then(|e| e.control.as_ref()) {
            Some(Control::Dropdown(state)) => state.selected,
            _ => 0,
        }
    }

    pub fn set_selected_option(&mut self, dropdown: NodeId, index: usize) {
        let Some(Control::Dropdown(state)) = self.element_mut(dropdown).and_then(|e| e.control.as_mut()) else {
            return;
        };
        state.selected = index.min(state.options.len().saturating_sub(1));
        let text = state.options.get(state.selected).cloned().unwrap_or_default();
        let label = NodeId(state.label);
        self.set_text(label, &text);
    }

    /// Whether a dropdown's list is open.
    pub fn is_dropdown_open(&self, dropdown: NodeId) -> bool {
        matches!(self.element(dropdown).and_then(|e| e.control.as_ref()), Some(Control::Dropdown(state)) if state.popup.is_some())
    }

    /// Opens a dropdown's list under it, in the layer above everything.
    pub fn open_dropdown(&mut self, dropdown: NodeId) {
        self.close_popup();
        let Some(Control::Dropdown(state)) = self.element(dropdown).and_then(|e| e.control.as_ref()) else { return };
        let (options, selected) = (state.options.clone(), state.selected);
        let [x, y, width, height] = self.rect(dropdown);
        let layer = self.popup_layer();
        let mut style = Style::column();
        style.layout.position = Position::Absolute;
        style.layout.inset.left = LengthPercentageAuto::length(x);
        style.layout.inset.top = LengthPercentageAuto::length(y + height + 4.0);
        style.layout.size.width = Dimension::length(width);
        let popup = self.panel(layer, style);
        self.add_class(popup, "dropdown-popup");
        // It lives in the top layer, so it takes the dropdown's text rather than the screen's.
        if let Some(text) = self.text_style(dropdown) {
            self.set_text_style(popup, text);
        }
        for (index, option) in options.iter().enumerate() {
            let row = Element {
                control: Some(Control::Option { dropdown: dropdown.0, index }),
                ..Element::new(Kind::Panel, Style::row())
            };
            let row = self.add(popup, row);
            self.add_class(row, "dropdown-option");
            self.set_checked(row, index == selected);
            self.add(row, Element { text: Some(Text::new(option)), ..Element::new(Kind::Label, Style::default()) });
        }
        if let Some(Control::Dropdown(state)) = self.element_mut(dropdown).and_then(|e| e.control.as_mut()) {
            state.popup = Some(popup.0);
        }
        self.open_popup = Some((dropdown.0, popup.0));
    }

    /// Closes whatever popup is open.
    pub fn close_popup(&mut self) {
        let Some((dropdown, popup)) = self.open_popup.take() else { return };
        if let Some(Control::Dropdown(state)) = self.element_mut(NodeId(dropdown)).and_then(|e| e.control.as_mut()) {
            state.popup = None;
        }
        self.remove(NodeId(popup));
    }

    /// The layer popups open in: over the whole screen, letting the pointer
    /// through, and kept last so it draws above everything.
    fn popup_layer(&mut self) -> NodeId {
        let layer = match self.popups {
            Some(layer) if self.tree.parent(layer) == Some(self.root) => NodeId(layer),
            _ => {
                let layer = self.panel(self.root(), Style::cover());
                self.add_class(layer, "popup-layer");
                self.popups = Some(layer.0);
                layer
            }
        };
        let _ = self.tree.remove_child(self.root, layer.0);
        let _ = self.tree.add_child(self.root, layer.0);
        layer
    }

    /// A press while a popup is open: on an option it is left to the click,
    /// and anywhere else closes the popup and is used up.
    pub(crate) fn press_popup(&mut self, path: &[taffy::NodeId]) -> bool {
        let Some((dropdown, popup)) = self.open_popup else { return false };
        if path.contains(&popup) {
            return false;
        }
        self.close_popup();
        // Pressing the dropdown itself just closes it, rather than opening it again.
        let _ = dropdown;
        true
    }

    /// Clicks on tabs, dropdowns and their options.
    pub(crate) fn click_panes(&mut self, target: taffy::NodeId) {
        let mut at = Some(target);
        while let Some(node) = at {
            let control = self.tree.get_node_context(node).and_then(|e| e.control.as_ref());
            match control {
                Some(Control::Tab { tabs, index }) => {
                    let (tabs, index) = (NodeId(*tabs), *index);
                    if self.selected_tab(tabs) != index {
                        self.set_selected_tab(tabs, index);
                        self.mark_changed(tabs);
                    }
                    return;
                }
                Some(Control::Option { dropdown, index }) => {
                    let (dropdown, index) = (NodeId(*dropdown), *index);
                    self.choose(dropdown, index);
                    self.close_popup();
                    return;
                }
                Some(Control::Dropdown(_)) => {
                    self.open_dropdown(NodeId(node));
                    return;
                }
                _ => {}
            }
            at = self.tree.parent(node);
        }
    }

    /// The user picked an option.
    fn choose(&mut self, dropdown: NodeId, index: usize) {
        if self.selected_option(dropdown) != index {
            self.set_selected_option(dropdown, index);
            self.mark_changed(dropdown);
        }
    }

    /// Keys on a focused dropdown: Enter or Space opens and closes it, the
    /// arrows move the choice, Escape closes it.
    pub(crate) fn dropdown_key(&mut self, dropdown: NodeId, key: Key) -> bool {
        let selected = self.selected_option(dropdown);
        match key {
            Key::Enter | Key::Space if self.is_dropdown_open(dropdown) => self.close_popup(),
            Key::Enter | Key::Space => self.open_dropdown(dropdown),
            Key::Escape if self.is_dropdown_open(dropdown) => self.close_popup(),
            Key::Up | Key::Left => self.choose(dropdown, selected.saturating_sub(1)),
            Key::Down | Key::Right => self.choose(dropdown, selected + 1),
            _ => return false,
        }
        if self.is_dropdown_open(dropdown) {
            self.open_dropdown(dropdown);
        }
        true
    }

    /// Starts dragging a split view's divider, if the press is on one.
    pub(crate) fn press_divider(&mut self, path: &[taffy::NodeId], point: (f32, f32)) -> bool {
        let divider = path.iter().rev().find_map(|&n| match self.tree.get_node_context(n)?.control.as_ref()? {
            Control::Divider(split) => Some((n, *split)),
            _ => None,
        });
        let Some((divider, split)) = divider else { return false };
        let [x, y, ..] = self.rect(NodeId(divider));
        if let Some(Control::Split(state)) = self.element_mut(NodeId(split)).and_then(|e| e.control.as_mut()) {
            state.grab = if state.row { point.0 - x } else { point.1 - y };
        }
        self.input.dragging = Some(split);
        true
    }

    pub(crate) fn drag_divider(&mut self, split: NodeId, point: (f32, f32)) {
        let Some(Control::Split(state)) = self.element(split).and_then(|e| e.control.as_ref()) else { return };
        let (row, grab) = (state.row, state.grab);
        let [x, y, ..] = self.content_box(split);
        let size = if row { point.0 - x - grab } else { point.1 - y - grab };
        if (size - self.split_size(split)).abs() > 0.01 {
            self.set_split(split, size);
            self.mark_changed(split);
        }
    }
}
