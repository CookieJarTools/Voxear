//! Scrolling: elements with `overflow: scroll` (and every scroll, list and
//! tree view) move their children by an offset, clipped to their padding
//! box. The wheel scrolls the innermost one under the pointer that can still
//! move that way, and a thin scrollbar floats over the right edge, as in
//! Linear and macOS, to drag or click.

use crate::{NodeId, Overflow, Ui};

/// The scrollbar thumb's width, and its gap from the edge, in logical pixels.
pub(crate) const BAR: f32 = 6.0;
pub(crate) const BAR_INSET: f32 = 2.0;
/// A thumb never gets shorter than this, so it stays easy to grab.
const MIN_THUMB: f32 = 24.0;

/// Where the vertical thumb sits along a track the height of the viewport:
/// its top and length. `None` when there is nothing to scroll.
pub(crate) fn thumb(viewport: f32, max: f32, offset: f32) -> Option<(f32, f32)> {
    if max <= 0.5 || viewport <= 0.0 {
        return None;
    }
    let length = (viewport * viewport / (viewport + max)).clamp(MIN_THUMB.min(viewport), viewport);
    Some(((viewport - length) * (offset / max).clamp(0.0, 1.0), length))
}

impl Ui {
    /// Scrolls the innermost scrolling element under the pointer that can
    /// still move this way. `dx` and `dy` are logical pixels, positive
    /// towards the end (right, down). Returns whether anything scrolled, so
    /// the game can leave the wheel alone.
    pub fn pointer_scroll(&mut self, dx: f32, dy: f32) -> bool {
        let path = self.hit_path();
        for &node in path.iter().rev() {
            if self.scroll_field(NodeId(node), dy) {
                return true;
            }
            let Some(element) = self.element(NodeId(node)) else { continue };
            if element.computed.overflow != Overflow::Scroll {
                continue;
            }
            let (offset, max) = (element.scroll, element.scroll_max);
            let moved = [(offset[0] + dx).clamp(0.0, max[0]), (offset[1] + dy).clamp(0.0, max[1])];
            if moved != offset {
                self.set_scroll(NodeId(node), moved);
                return true;
            }
        }
        false
    }

    /// How far an element's children can scroll, in logical pixels, as of
    /// the last layout.
    pub fn scroll_max(&self, node: NodeId) -> [f32; 2] {
        self.element(node).map_or([0.0; 2], |e| e.scroll_max)
    }

    /// How far an element's children are scrolled, in logical pixels.
    pub fn scroll_offset(&self, node: NodeId) -> [f32; 2] {
        self.element(node).map_or([0.0; 2], |e| e.scroll)
    }

    /// Scrolls to `offset`, within what the last layout allows.
    pub fn set_scroll(&mut self, node: NodeId, offset: [f32; 2]) {
        let Some(element) = self.element_mut(node) else { return };
        let max = element.scroll_max;
        element.scroll = [offset[0].clamp(0.0, max[0]), offset[1].clamp(0.0, max[1])];
        self.update_hover();
    }

    /// Recomputes how far each scrolling element can scroll, once laid out.
    pub(crate) fn update_scroll_limits(&mut self) {
        for node in self.scrollers.clone() {
            let Ok(layout) = self.tree.layout(node).copied() else { continue };
            let viewport = [
                layout.size.width - layout.border.left - layout.border.right,
                layout.size.height - layout.border.top - layout.border.bottom,
            ];
            let content = [layout.scrollable_overflow_rect.right, layout.scrollable_overflow_rect.bottom];
            let Some(element) = self.tree.get_node_context_mut(node) else { continue };
            element.scroll_max = [(content[0] - viewport[0]).max(0.0), (content[1] - viewport[1]).max(0.0)];
            element.scroll =
                [element.scroll[0].min(element.scroll_max[0]), element.scroll[1].min(element.scroll_max[1])];
        }
    }

    /// An element's padding box on screen, where its children show and its
    /// scrollbar runs: left, top, width, height.
    pub fn viewport(&self, node: NodeId) -> [f32; 4] {
        let [x, y, w, h] = self.rect(node);
        let Ok(layout) = self.tree.layout(node.0) else { return [x, y, w, h] };
        let border = layout.border;
        [x + border.left, y + border.top, w - border.left - border.right, h - border.top - border.bottom]
    }

    /// For a press at `point`: the scrolling element whose scrollbar track
    /// is there, and where its thumb is (top, length) in screen pixels.
    pub(crate) fn scrollbar_at(&self, point: (f32, f32)) -> Option<(NodeId, f32, f32)> {
        let path = self.hit_path();
        path.iter().rev().find_map(|&node| {
            let element = self.element(NodeId(node))?;
            if element.computed.overflow != Overflow::Scroll {
                return None;
            }
            let [x, y, w, h] = self.viewport(NodeId(node));
            let (top, length) = thumb(h, element.scroll_max[1], element.scroll[1])?;
            let track = x + w - BAR - 2.0 * BAR_INSET;
            let on_track = (track..x + w).contains(&point.0) && (y..y + h).contains(&point.1);
            on_track.then_some((NodeId(node), y + top, length))
        })
    }

    /// Scrolls so the thumb's top is at `top` on screen.
    pub(crate) fn drag_thumb(&mut self, node: NodeId, top: f32) {
        let Some(element) = self.element(node) else { return };
        let max = element.scroll_max[1];
        let [_, y, _, h] = self.viewport(node);
        let Some((_, length)) = thumb(h, max, 0.0) else { return };
        let fraction = ((top - y) / (h - length).max(1.0)).clamp(0.0, 1.0);
        let offset = [element.scroll[0], fraction * max];
        self.set_scroll(node, offset);
    }
}
