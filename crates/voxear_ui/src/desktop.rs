//! Desktop windows: parts of the tree shown in windows of their own beside
//! the main one, as a panel the editor floats out of its window is.
//!
//! Such an element stays in the tree, styled and laid out with everything
//! else, so nothing in it is rebuilt or loses its state as it moves out and
//! back. It is placed at the screen's top left at its window's size, so its
//! own coordinates are its window's: [`Ui::rect`] of anything in it, and
//! the pointer in that window, need no converting. It is left out of
//! [`Ui::frame`] and drawn by [`Ui::window_frame`], and the pointer finds
//! it only while [`Ui::set_pointer_window`] says the pointer is in its
//! window, where it finds nothing else. Popups opened while the pointer is
//! there, as a menu from its tab, open in it.
//!
//! The dock makes them (see [`crate::DockWindow`]); a window is known by the
//! [`DesktopWindow`] it was given, which outlives the elements built for it.

use crate::{NodeId, Style, Ui, UiFrame, draw::Draw};

/// A desktop window the UI shows part of itself in, beside its main one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DesktopWindow(pub u32);

/// An element shown in a desktop window, with the window's size in logical
/// pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shown {
    pub window: DesktopWindow,
    pub node: taffy::NodeId,
    pub size: (f32, f32),
}

impl Ui {
    /// Shows `node` in a desktop window of its own, `size` logical pixels,
    /// or back in the main window with `None`. The element should be placed
    /// out of the flow (`position: absolute`), as the dock's floating
    /// windows are.
    pub(crate) fn show_in_window(&mut self, node: NodeId, window: Option<(DesktopWindow, (f32, f32))>) {
        self.desktop.retain(|shown| shown.node != node.0);
        if let Some((window, size)) = window {
            self.desktop.retain(|shown| shown.window != window);
            self.desktop.push(Shown { window, node: node.0, size });
            self.place_in_window(node.0, size);
        }
    }

    /// The desktop windows parts of the UI are shown in.
    pub fn desktop_windows(&self) -> Vec<DesktopWindow> {
        self.desktop.iter().map(|shown| shown.window).collect()
    }

    /// The desktop window `node` is shown in, or `None` for the main one.
    pub fn window_of(&self, node: NodeId) -> Option<DesktopWindow> {
        let shown = self.desktop.iter().find(|shown| self.is_within(node, NodeId(shown.node)))?;
        Some(shown.window)
    }

    /// The element shown in a desktop window.
    pub fn window_element(&self, window: DesktopWindow) -> Option<NodeId> {
        self.window_node(window).map(NodeId)
    }

    pub(crate) fn window_node(&self, window: DesktopWindow) -> Option<taffy::NodeId> {
        self.desktop.iter().find(|shown| shown.window == window).map(|shown| shown.node)
    }

    /// Says which window the pointer is in, `None` for the main one, before
    /// its moves and presses there: it then finds only what that window
    /// shows, and its positions are from that window's top left.
    pub fn set_pointer_window(&mut self, window: Option<DesktopWindow>) {
        if self.pointer_window != window {
            self.pointer_window = window;
            self.update_hover();
        }
    }

    /// The window the pointer is in, `None` for the main one.
    pub fn pointer_window(&self) -> Option<DesktopWindow> {
        self.pointer_window
    }

    /// A desktop window whose bar was pressed since the last call, to be
    /// moved by the desktop, which alone knows where windows are. Hand it
    /// over while the button is still down.
    pub fn take_window_move(&mut self) -> Option<DesktopWindow> {
        self.window_move.take()
    }

    /// The size of the screen `node` is on, in logical pixels: its desktop
    /// window's, or the main window's.
    pub(crate) fn screen_size_of(&self, node: NodeId) -> (f32, f32) {
        let shown = self.desktop.iter().find(|shown| self.is_within(node, NodeId(shown.node)));
        shown.map_or((self.viewport.0, self.viewport.1), |shown| shown.size)
    }

    /// Whether `node` is the element of a desktop window that the pointer
    /// is not in, which the pointer then passes by.
    pub(crate) fn hidden_from_pointer(&self, node: taffy::NodeId) -> bool {
        self.desktop.iter().any(|shown| shown.node == node && Some(shown.window) != self.pointer_window)
    }

    /// Where the elements in `node`'s parent are placed from, on screen.
    pub(crate) fn parent_origin(&self, node: taffy::NodeId) -> (f32, f32) {
        let [x, y, ..] = self.rect(NodeId(node));
        let location = self.tree.layout(node).map(|l| l.location).unwrap_or_default();
        (x - location.x, y - location.y)
    }

    /// Moves a desktop window's element to the screen's top left at its
    /// window's size, from where the last layout put it. Returns whether it
    /// moved, which then needs laying out again.
    fn place_in_window(&mut self, node: taffy::NodeId, (width, height): (f32, f32)) -> bool {
        let [x, y, w, h] = self.rect(NodeId(node));
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        if close(x, 0.0) && close(y, 0.0) && close(w, width) && close(h, height) {
            return false;
        }
        let Some(element) = self.element(NodeId(node)) else { return false };
        let mut style: Style = element.base.clone();
        let inset = |value: taffy::LengthPercentageAuto| value.resolve_to_option(0.0, |_, _| 0.0).unwrap_or(0.0);
        let (left, top) = (inset(style.layout.inset.left), inset(style.layout.inset.top));
        style.layout.inset.left = taffy::LengthPercentageAuto::length(left - x);
        style.layout.inset.top = taffy::LengthPercentageAuto::length(top - y);
        style.layout.size =
            taffy::Size { width: taffy::Dimension::length(width), height: taffy::Dimension::length(height) };
        self.set_style(NodeId(node), style);
        true
    }

    /// Places every desktop window's element after a layout; see
    /// [`place_in_window`](Self::place_in_window).
    pub(crate) fn place_windows(&mut self) -> bool {
        let shown = self.desktop.clone();
        let mut moved = false;
        for shown in shown {
            moved |= self.place_in_window(shown.node, shown.size);
        }
        moved
    }

    /// Gives a desktop window a new size in logical pixels, as when the
    /// desktop resized it.
    pub(crate) fn resize_window(&mut self, window: DesktopWindow, size: (f32, f32)) {
        let Some(shown) = self.desktop.iter_mut().find(|shown| shown.window == window) else { return };
        shown.size = size;
        let node = shown.node;
        self.place_in_window(node, size);
    }

    /// What to draw in a desktop window this frame, as [`frame`](Self::frame)
    /// gives the main window's. Call it after `frame`, which lays out and
    /// styles the windows too; its glyphs come with an atlas of their own.
    pub fn window_frame(&mut self, window: DesktopWindow) -> UiFrame {
        let Some(node) = self.window_node(window) else { return UiFrame::default() };
        let parent = self.parent_origin(node);
        let (scale, focused, caret_on, time) = (self.viewport.2, self.input.focused, self.caret_on(), self.input.time);
        let mut quads = Vec::new();
        let (mut values, mut unknown) = (Vec::new(), Vec::new());
        for _ in 0..2 {
            quads.clear();
            values.clear();
            self.atlas.restarted = false;
            let mut draw = Draw {
                tree: &mut self.tree,
                atlas: &mut self.atlas,
                scale,
                quads: &mut quads,
                focused,
                caret_on,
                time,
                raised: Vec::new(),
                lifting: false,
                skip: Vec::new(),
                shaders: &self.shaders,
                values: &mut values,
                unknown_shaders: &mut unknown,
            };
            draw.subtree(node, parent);
            if !self.atlas.restarted {
                break;
            }
        }
        self.warn_unknown_shaders(unknown);
        UiFrame { quads, values, atlas: self.atlas.snapshot() }
    }

    /// The size of the screen an element is on, in logical pixels: its
    /// desktop window's, or the main window's.
    pub fn screen_size(&self, node: NodeId) -> (f32, f32) {
        self.screen_size_of(node)
    }

    /// Moves `node` over a desktop window, into the layer its popups open
    /// in, or with `None` back where it was before it first moved: for an
    /// element of the caller's own placed over what opened it, as a picker
    /// under a field of a panel floated out. A desktop window built again,
    /// or taken away, sends it back first.
    pub fn show_over_window(&mut self, node: NodeId, window: Option<DesktopWindow>) {
        if self.window_of(node) == window {
            return;
        }
        let home = self.visitors.iter().position(|&(visitor, _)| visitor == node.0);
        match (window, home) {
            (Some(window), _) => {
                if home.is_none()
                    && let Some(parent) = self.tree.parent(node.0)
                {
                    self.visitors.push((node.0, parent));
                }
                let layer = self.popup_layer_in(Some(window));
                self.move_into(node, layer);
            }
            (None, Some(index)) => {
                let (_, home) = self.visitors.remove(index);
                self.move_into(node, NodeId(home));
            }
            (None, None) => {}
        }
    }

    /// Sends what [`show_over_window`](Self::show_over_window) moved into
    /// `node` back where it came from, before `node` is taken out.
    pub(crate) fn send_visitors_home(&mut self, node: taffy::NodeId) {
        let leaving: Vec<(taffy::NodeId, taffy::NodeId)> = self
            .visitors
            .iter()
            .copied()
            .filter(|&(visitor, home)| {
                self.is_within(NodeId(visitor), NodeId(node)) && !self.is_within(NodeId(home), NodeId(node))
            })
            .collect();
        for (visitor, home) in leaving {
            self.visitors.retain(|&(v, _)| v != visitor);
            self.move_into(NodeId(visitor), NodeId(home));
        }
    }

    /// The layer popups open in on the screen the pointer is on, as a menu
    /// opened from a tab in a desktop window opens in that window.
    pub(crate) fn popup_layer_here(&mut self) -> NodeId {
        self.popup_layer_in(self.pointer_window)
    }

    /// The layer popups open in on a desktop window, or the main window's:
    /// over the whole window, letting the pointer through, and kept last so
    /// it draws above everything there.
    pub(crate) fn popup_layer_in(&mut self, window: Option<DesktopWindow>) -> NodeId {
        let Some(node) = window.and_then(|window| self.window_node(window)) else { return self.popup_layer() };
        let found = self.tree.children(node).unwrap_or_default().into_iter().map(NodeId);
        let found = found.filter(|&child| self.has_class(child, "popup-layer")).last();
        let layer = found.unwrap_or_else(|| {
            let layer = self.panel(NodeId(node), Style::cover());
            self.add_class(layer, "popup-layer");
            layer
        });
        let _ = self.tree.remove_child(node, layer.0);
        let _ = self.tree.add_child(node, layer.0);
        layer
    }
}
