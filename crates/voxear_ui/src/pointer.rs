//! The pointer: which elements it is over (`:hover`), which it holds down
//! (`:active`), and clicks. Hit testing uses the last frame's layout.

use crate::{NodeId, Ui, selector::States};

#[derive(Default)]
pub(crate) struct Pointer {
    /// In logical pixels; `None` once it leaves the window.
    position: Option<(f32, f32)>,
    /// The element under the pointer and everything it is in.
    hovered: Vec<taffy::NodeId>,
    /// What the primary button went down on, and everything it is in.
    pressed: Vec<taffy::NodeId>,
    /// Elements clicked since the last frame.
    pub clicks: Vec<taffy::NodeId>,
}

impl Pointer {
    /// Drops a removed element from everything that refers to it.
    pub fn forget(&mut self, node: taffy::NodeId) {
        for list in [&mut self.hovered, &mut self.pressed, &mut self.clicks] {
            list.retain(|&n| n != node);
        }
    }
}

impl Ui {
    /// The pointer moved to `x`, `y`, in physical pixels from the window's
    /// top left.
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        let scale = if self.viewport.2 > 0.0 { self.viewport.2 } else { 1.0 };
        self.pointer.position = Some((x / scale, y / scale));
        self.update_hover();
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
        let path = self.hit_path();
        if pressed {
            self.set_states(&path, States::ACTIVE, true);
            self.pointer.pressed = path.clone();
        } else {
            let pressed = std::mem::take(&mut self.pointer.pressed);
            self.set_states(&pressed, States::ACTIVE, false);
            if let Some(&target) = pressed.last()
                && path.contains(&target)
                && !self.disabled(&pressed)
            {
                self.pointer.clicks.push(target);
            }
        }
        !path.is_empty()
    }

    /// Whether the pointer is over an element that takes it.
    pub fn wants_pointer(&self) -> bool {
        !self.pointer.hovered.is_empty()
    }

    pub fn is_hovered(&self, node: NodeId) -> bool {
        self.pointer.hovered.contains(&node.0)
    }

    /// Whether `node`, or anything in it, was clicked since the last frame.
    pub fn clicked(&self, node: NodeId) -> bool {
        self.pointer.clicks.iter().any(|&target| self.is_within(NodeId(target), node))
    }

    fn update_hover(&mut self) {
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
    fn hit_path(&self) -> Vec<taffy::NodeId> {
        let Some(point) = self.pointer.position else { return Vec::new() };
        let Some(target) = self.hit(self.root, (0.0, 0.0), point) else { return Vec::new() };
        let mut path = vec![target];
        while let Some(parent) = self.tree.parent(*path.last().expect("starts with one")) {
            path.push(parent);
        }
        path.reverse();
        path
    }

    fn hit(&self, node: taffy::NodeId, parent: (f32, f32), point: (f32, f32)) -> Option<taffy::NodeId> {
        let element = self.tree.get_node_context(node)?;
        if element.computed.layout.display == taffy::Display::None {
            return None;
        }
        let layout = self.tree.layout(node).ok()?;
        let at = (parent.0 + layout.location.x, parent.1 + layout.location.y);
        // Later children draw on top, so they are hit first.
        let children = self.tree.children(node).unwrap_or_default();
        if let Some(found) = children.into_iter().rev().find_map(|child| self.hit(child, at, point)) {
            return Some(found);
        }
        let inside =
            (at.0..at.0 + layout.size.width).contains(&point.0) && (at.1..at.1 + layout.size.height).contains(&point.1);
        (inside && element.computed.pickable).then_some(node)
    }
}
