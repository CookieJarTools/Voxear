//! The pointer: which elements it is over (`:hover`), which it holds down
//! (`:active`), clicks, and dragging scrollbars. Hit testing uses the last
//! frame's layout, scrolled and clipped as it was drawn.

use crate::{NodeId, Overflow, Ui, selector::States};

/// How far apart, in logical pixels, a press and its release may be and
/// still click what was built again in between, and two clicks may be and
/// still be a double click.
const CLICK_SLOP: f32 = 5.0;

/// Keys held with the pointer, which change what a click on a list does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
}

#[derive(Default)]
pub(crate) struct Pointer {
    /// In logical pixels; `None` once it leaves the window.
    position: Option<(f32, f32)>,
    /// The element under the pointer and everything it is in.
    hovered: Vec<taffy::NodeId>,
    /// What the primary button went down on, and everything it is in.
    pub(crate) pressed: Vec<taffy::NodeId>,
    /// Where it went down, and whether what it went down on was taken out
    /// since, as when a panel is built again under the pointer.
    press_at: Option<(f32, f32)>,
    press_lost: bool,
    /// Elements clicked since the last frame.
    pub clicks: Vec<taffy::NodeId>,
    /// Elements clicked twice in quick succession since the last frame.
    pub double_clicks: Vec<taffy::NodeId>,
    /// Links clicked in rich text since the last frame, with the label.
    pub link_clicks: Vec<(taffy::NodeId, String)>,
    /// What the secondary button went down on, and everything it is in, and
    /// elements context clicked since the last frame.
    context_pressed: Vec<taffy::NodeId>,
    pub context_clicks: Vec<taffy::NodeId>,
    /// Where and when the last click was, to tell a double click.
    last_click: Option<((f32, f32), f32)>,
    /// A scrollbar being dragged: its element, and where on the thumb it was
    /// grabbed.
    dragging: Option<(NodeId, f32)>,
}

impl Pointer {
    /// Drops a removed element from everything that refers to it.
    pub fn forget(&mut self, node: taffy::NodeId) {
        self.press_lost |= self.pressed.contains(&node);
        let lists = [
            &mut self.hovered,
            &mut self.pressed,
            &mut self.clicks,
            &mut self.double_clicks,
            &mut self.context_pressed,
            &mut self.context_clicks,
        ];
        for list in lists {
            list.retain(|&n| n != node);
        }
        self.link_clicks.retain(|(n, _)| *n != node);
    }
}

impl Ui {
    /// The pointer moved to `x`, `y`, in physical pixels from the window's
    /// top left.
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        let scale = if self.viewport.2 > 0.0 { self.viewport.2 } else { 1.0 };
        let point = (x / scale, y / scale);
        self.pointer.position = Some(point);
        if self.is_scrubbing() {
            self.drag_scrub(point);
            return self.update_hover();
        }
        if let Some((node, grab)) = self.pointer.dragging {
            self.drag_thumb(node, point.1 - grab);
        }
        if self.move_reorder(point) {
            return self.update_hover();
        }
        self.drag_controls(point);
        self.drag_dock(point);
        self.move_drag(point);
        self.update_hover();
    }

    /// Which keys are held, for what clicks on lists do.
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    /// Which keys are held, as a click on tiles of one's own reads them.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// The pointer left the window.
    pub fn pointer_leave(&mut self) {
        self.pointer.position = None;
        self.update_hover();
    }

    /// The primary button went down or up. A press and release on the same
    /// element is a click on it. Returns whether the UI is under the pointer,
    /// so the game can leave the press alone.
    pub fn pointer_button(&mut self, pressed: bool) -> bool {
        if !pressed && self.pointer.dragging.take().is_some() {
            return true;
        }
        if !pressed && self.release_scrub() {
            self.pointer_cancel();
            return true;
        }
        if !pressed {
            self.input.dragging = None;
            if self.release_reorder() || self.release_dock() || self.release_drag() {
                return true;
            }
        }
        if pressed
            && let Some(point) = self.pointer.position
            && let Some((node, top, length)) = self.scrollbar_at(point)
        {
            // On the thumb it is grabbed where it was pressed; on the track it jumps to centre there.
            let grab = if (top..top + length).contains(&point.1) { point.1 - top } else { length / 2.0 };
            self.drag_thumb(node, point.1 - grab);
            self.pointer.dragging = Some((node, grab));
            return true;
        }
        let path = self.hit_path();
        if pressed && self.press_popup(&path) {
            return true;
        }
        if pressed
            && let Some(point) = self.pointer.position
            && self.press_divider(&path, point)
        {
            return true;
        }
        if pressed
            && let Some(point) = self.pointer.position
            && self.press_scrub(&path, point)
        {
            self.set_states(&path, States::ACTIVE, true);
            self.pointer.pressed = path;
            return true;
        }
        if pressed {
            self.set_states(&path, States::ACTIVE, true);
            self.pointer.pressed = path.clone();
            (self.pointer.press_at, self.pointer.press_lost) = (self.pointer.position, false);
            if let Some(point) = self.pointer.position {
                self.press_controls(&path, point);
                self.press_dock(&path, point);
                if !self.press_reorder(&path, point) {
                    self.press_drag(&path, point);
                }
            }
        } else {
            let pressed = std::mem::take(&mut self.pointer.pressed);
            self.set_states(&pressed, States::ACTIVE, false);
            if let Some(target) = self.click_target(&pressed, &path) {
                self.pointer.clicks.push(target);
                // As desktops tell one: a second click soon after, in about the same place, on
                // whatever is there now, since the first may have built it again.
                let (time, here) = (self.input.time, self.pointer.position.unwrap_or_default());
                let twice = self.pointer.last_click.is_some_and(|(at, when)| {
                    time - when <= crate::input::MULTI_CLICK && (at.0 - here.0).hypot(at.1 - here.1) <= CLICK_SLOP
                });
                if twice {
                    self.pointer.double_clicks.push(target);
                }
                self.pointer.last_click = (!twice).then_some((here, time));
                if let Some(point) = self.pointer.position
                    && let Some(url) = self.link_at(NodeId(target), point)
                {
                    self.pointer.link_clicks.push((target, url));
                }
                self.row_clicked(target);
                self.click_controls(target);
                self.click_panes(target);
            }
        }
        !path.is_empty()
    }

    /// The secondary button went down or up. A press and release on the same
    /// element is a context click on it, which on a list row not selected
    /// selects it first. Returns whether the UI is under the pointer.
    pub fn pointer_secondary(&mut self, pressed: bool) -> bool {
        let path = self.hit_path();
        if pressed {
            if self.press_popup(&path) {
                return true;
            }
            self.pointer.context_pressed = path.clone();
            return !path.is_empty();
        }
        let pressed = std::mem::take(&mut self.pointer.context_pressed);
        if let Some(&target) = pressed.last()
            && path.contains(&target)
            && !self.disabled(&pressed)
        {
            self.pointer.context_clicks.push(target);
            self.row_context_clicked(target);
        }
        !path.is_empty()
    }

    /// Whether `node`, or anything in it, was context clicked since the last
    /// frame.
    pub fn context_clicked(&self, node: NodeId) -> bool {
        self.pointer.context_clicks.iter().any(|&target| self.is_within(NodeId(target), node))
    }

    /// Where the pointer is, in logical pixels, while it is in the window.
    pub fn pointer_position(&self) -> Option<(f32, f32)> {
        self.pointer.position
    }

    /// Forgets the press in progress, so letting go is no click, as when the
    /// desktop took the press over to move a window.
    pub fn cancel_press(&mut self) {
        self.pointer_cancel();
        self.take_dock_drag();
        self.pointer.dragging = None;
    }

    /// Whether a press of this UI's is held: on one of its elements, or a
    /// drag it started. Its release is this UI's, wherever the pointer is.
    pub fn holds_press(&self) -> bool {
        !self.pointer.pressed.is_empty()
            || self.pointer.dragging.is_some()
            || self.scrub.is_some()
            || self.reorder_pressed()
            || self.drag.is_some()
            || self.dock_drag.is_some()
    }

    /// Forgets the press in progress, so letting go is no click: for a press
    /// that turned into a drag.
    pub(crate) fn pointer_cancel(&mut self) {
        let pressed = std::mem::take(&mut self.pointer.pressed);
        self.set_states(&pressed, States::ACTIVE, false);
    }

    /// What a release clicks: what the press went down on, if the pointer is
    /// still over it; or, when that was taken out and built again in the
    /// meantime, as a panel rebuilt by a value typed a moment before, what
    /// is under the pointer now, if it has hardly moved. Nothing disabled.
    fn click_target(&mut self, pressed: &[taffy::NodeId], path: &[taffy::NodeId]) -> Option<taffy::NodeId> {
        let from = self.pointer.press_at.take();
        if std::mem::take(&mut self.pointer.press_lost) {
            let (from, to) = (from?, self.pointer.position?);
            let still = (from.0 - to.0).hypot(from.1 - to.1) <= CLICK_SLOP;
            return path.last().copied().filter(|_| still && !self.disabled(path));
        }
        pressed.last().copied().filter(|target| path.contains(target) && !self.disabled(pressed))
    }

    /// Whether the pointer is over an element that takes it.
    pub fn wants_pointer(&self) -> bool {
        !self.pointer.hovered.is_empty()
    }

    /// The innermost element under the pointer.
    pub fn pointer_hovered(&self) -> Option<NodeId> {
        self.pointer.hovered.last().copied().map(NodeId)
    }

    pub fn is_hovered(&self, node: NodeId) -> bool {
        self.pointer.hovered.contains(&node.0)
    }

    /// The tooltip of the innermost element under the pointer that has one
    /// (see [`Ui::set_tooltip`]), and that element.
    pub fn hovered_tooltip(&self) -> Option<(String, NodeId)> {
        let tooltip = |node: &taffy::NodeId| Some((self.tooltips.get(node)?.0.clone(), NodeId(*node)));
        self.pointer.hovered.iter().rev().find_map(tooltip)
    }

    /// Whether the primary button is held down on `node`, or on anything in
    /// it, as for dragging on an element to turn what it shows.
    pub fn is_pressed(&self, node: NodeId) -> bool {
        self.pointer.pressed.iter().any(|&target| self.is_within(NodeId(target), node))
    }

    /// Whether `node`, or anything in it, was clicked since the last frame.
    pub fn clicked(&self, node: NodeId) -> bool {
        self.pointer.clicks.iter().any(|&target| self.is_within(NodeId(target), node))
    }

    /// Whether `node`, or anything in it, was clicked a second time in
    /// quick succession since the last frame, as opening a folder takes.
    pub fn double_clicked(&self, node: NodeId) -> bool {
        self.pointer.double_clicks.iter().any(|&target| self.is_within(NodeId(target), node))
    }

    pub(crate) fn update_hover(&mut self) {
        let path = self.hit_path();
        let left: Vec<taffy::NodeId> = self.pointer.hovered.iter().copied().filter(|n| !path.contains(n)).collect();
        self.set_states(&left, States::HOVER, false);
        self.set_states(&path, States::HOVER, true);
        self.pointer.hovered = path;
    }

    fn set_states(&mut self, nodes: &[taffy::NodeId], state: States, on: bool) {
        for &node in nodes {
            self.set_state(NodeId(node), state, on);
        }
    }

    fn disabled(&self, path: &[taffy::NodeId]) -> bool {
        path.iter().any(|&n| self.element(NodeId(n)).is_some_and(|e| e.states.contains(States::DISABLED)))
    }

    /// The topmost pickable element under the pointer and everything it is
    /// in, from the root down; empty when there is none.
    /// In a desktop window, only what it shows is found.
    pub(crate) fn hit_path(&self) -> Vec<taffy::NodeId> {
        let Some(point) = self.pointer.position else { return Vec::new() };
        let found = match self.pointer_window {
            Some(window) => self.window_node(window).and_then(|node| self.hit(node, self.parent_origin(node), point)),
            None => self.hit(self.root, (0.0, 0.0), point),
        };
        let Some(target) = found else { return Vec::new() };
        let mut path = vec![target];
        while let Some(parent) = self.tree.parent(*path.last().expect("starts with one")) {
            path.push(parent);
        }
        path.reverse();
        path
    }

    fn hit(&self, node: taffy::NodeId, parent: (f32, f32), point: (f32, f32)) -> Option<taffy::NodeId> {
        let element = self.tree.get_node_context(node)?;
        if element.computed.layout.display == taffy::Display::None || self.hidden_from_pointer(node) {
            return None;
        }
        let layout = self.tree.layout(node).ok()?;
        let at = (parent.0 + layout.location.x, parent.1 + layout.location.y);
        let inside =
            (at.0..at.0 + layout.size.width).contains(&point.0) && (at.1..at.1 + layout.size.height).contains(&point.1);
        // Children it clips can only be hit inside it.
        if inside || element.computed.overflow == Overflow::Visible {
            let origin = (at.0 - element.scroll[0], at.1 - element.scroll[1]);
            // Later children draw on top, so they are hit first.
            let children = self.tree.children(node).unwrap_or_default();
            if let Some(found) = children.into_iter().rev().find_map(|child| self.hit(child, origin, point)) {
                return Some(found);
            }
        }
        (inside && element.computed.pickable).then_some(node)
    }
}
