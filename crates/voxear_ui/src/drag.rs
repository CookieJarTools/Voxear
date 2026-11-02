//! Dragging: list rows, and elements made draggable, onto elements that
//! take drops.
//!
//! A press that moves a few pixels picks up what was pressed: a row of a
//! list whose rows drag ([`Ui::set_rows_draggable`]), or else the nearest
//! element made draggable ([`Ui::set_draggable`]). A `drag-ghost` with its
//! text follows the pointer, and an element that takes drops
//! ([`Ui::accept_drops`]) is marked `drop-hover` while the pointer is over
//! it. Letting go on one is a drop onto it, which [`Ui::dropped_onto`]
//! reports with what was dragged; letting go elsewhere in a row's own list
//! is a drop in that list (see `list`). A list whose rows take drops also
//! marks and reports the row something from elsewhere lands on, while its
//! own rows still land in it as rows. Either way the release is no click.

use taffy::{LengthPercentageAuto, Position};

use crate::{
    NodeId, Style, Ui,
    tree::{Element, Kind, Text},
};

/// How far a press must move to be a drag rather than a click.
const DRAG_START: f32 = 4.0;
/// Where the ghost sits from the pointer, so it does not hide what is under it.
const GHOST_OFFSET: (f32, f32) = (14.0, 10.0);

/// What is being dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragSource {
    /// A list's row: the list, the row's index, and in a tree its item.
    Row { list: NodeId, index: usize, item: Option<u64> },
    /// An element made draggable.
    Element(NodeId),
}

/// A press that may become a drag: what was pressed, where, whether it has
/// moved far enough to be dragged, the row marked where it would land in
/// its own list, the element taking drops under the pointer, and the ghost.
pub(crate) struct Drag {
    source: DragSource,
    from: (f32, f32),
    moving: bool,
    over_row: Option<(taffy::NodeId, crate::list::DropPlace)>,
    over_target: Option<taffy::NodeId>,
    ghost: Option<taffy::NodeId>,
}

impl Ui {
    /// Lets an element be dragged onto elements that take drops.
    pub fn set_draggable(&mut self, node: NodeId, draggable: bool) {
        if draggable {
            self.draggables.insert(node.0);
        } else {
            self.draggables.remove(&node.0);
        }
    }

    /// Lets an element take drops of anything dragged.
    pub fn accept_drops(&mut self, node: NodeId, accept: bool) {
        if accept {
            self.drop_targets.insert(node.0);
        } else {
            self.drop_targets.remove(&node.0);
        }
    }

    /// What was dropped onto `target` since the last frame.
    pub fn dropped_onto(&self, target: NodeId) -> Option<DragSource> {
        self.element_drops.iter().find(|(at, _)| *at == target.0).map(|&(_, source)| source)
    }

    /// What is being dragged, once it has moved far enough to be.
    pub fn dragging(&self) -> Option<DragSource> {
        self.drag.as_ref().filter(|drag| drag.moving).map(|drag| drag.source)
    }

    /// A press, which becomes a drag if it was on a draggable row or element:
    /// the innermost, so a draggable tile in a row is dragged, not the row.
    pub(crate) fn press_drag(&mut self, path: &[taffy::NodeId], point: (f32, f32)) {
        let row = path.last().and_then(|&target| self.draggable_row(target));
        let row_depth = match row {
            Some(DragSource::Row { list, index, .. }) => {
                self.row_node(list, index).and_then(|n| path.iter().position(|p| *p == n))
            }
            _ => None,
        };
        let element = path.iter().rposition(|n| self.draggables.contains(n));
        let source = match (element, row) {
            (Some(depth), _) if row_depth.is_none_or(|row| depth > row) => DragSource::Element(NodeId(path[depth])),
            (_, Some(row)) => row,
            _ => return,
        };
        self.drag = Some(Drag { source, from: point, moving: false, over_row: None, over_target: None, ghost: None });
    }

    /// The pointer moved with something pressed: past a few pixels it is
    /// picked up, and what it would land on is marked.
    pub(crate) fn move_drag(&mut self, point: (f32, f32)) {
        let Some(drag) = &self.drag else { return };
        let (source, from, moving) = (drag.source, drag.from, drag.moving);
        if !moving && (point.0 - from.0).hypot(point.1 - from.1) < DRAG_START {
            return;
        }
        if !moving {
            // Letting go is a drop now, not a click.
            self.pointer_cancel();
            let ghost = self.ghost(source);
            if let Some(drag) = &mut self.drag {
                (drag.moving, drag.ghost) = (true, Some(ghost));
            }
            if let DragSource::Row { list, .. } = source {
                self.set_class(list, "dragging-rows", true);
            }
        }
        self.place_ghost(point);
        let target = self.target_under_pointer(source);
        let row = match (source, target) {
            (DragSource::Row { list, index, .. }, None) => self.landing_row(list, index),
            (_, Some(target)) => self.landing_from_elsewhere(NodeId(target)),
            (DragSource::Element(_), None) => None,
        };
        let (old_row, old_target) = self.drag.as_ref().map_or((None, None), |d| (d.over_row, d.over_target));
        self.mark_row(old_row, row);
        self.mark(old_target, target, "drop-hover");
        if let Some(drag) = &mut self.drag {
            (drag.over_row, drag.over_target) = (row, target);
        }
    }

    /// Lets go of what was pressed. Something being dragged lands on the
    /// element taking drops under the pointer, or in its own list; returns
    /// whether it was being dragged, as the release is then used up.
    pub(crate) fn release_drag(&mut self) -> bool {
        let Some(drag) = self.drag.take() else { return false };
        self.mark_row(drag.over_row, None);
        self.mark(drag.over_target, None, "drop-hover");
        if let Some(ghost) = drag.ghost {
            self.remove(NodeId(ghost));
        }
        if !drag.moving {
            return false;
        }
        if let DragSource::Row { list, .. } = drag.source {
            self.set_class(list, "dragging-rows", false);
        }
        if let Some(target) = self.target_under_pointer(drag.source) {
            self.element_drops.push((target, drag.source));
            self.drop_from_elsewhere(NodeId(target), drag.source);
            return true;
        }
        if let DragSource::Row { list, index, .. } = drag.source
            && self.hit_path().contains(&list.0)
        {
            self.drop_in_list(list, index);
        }
        true
    }

    /// The innermost element taking drops under the pointer, but for the
    /// list `source` is a row of, when its rows take drops: there it lands
    /// as a row.
    fn target_under_pointer(&self, source: DragSource) -> Option<taffy::NodeId> {
        let target = self.hit_path().into_iter().rev().find(|n| self.drop_targets.contains(n))?;
        let own = matches!(source, DragSource::Row { list, .. } if list.0 == target);
        (!own || !self.takes_row_drops(target)).then_some(target)
    }

    /// Moves the mark of where a row would land from one row to another.
    fn mark_row(
        &mut self,
        from: Option<(taffy::NodeId, crate::list::DropPlace)>,
        to: Option<(taffy::NodeId, crate::list::DropPlace)>,
    ) {
        if from == to {
            return;
        }
        if let Some((row, place)) = from {
            self.set_class(NodeId(row), place.class(), false);
        }
        if let Some((row, place)) = to {
            self.set_class(NodeId(row), place.class(), true);
        }
    }

    /// Moves a class from one element to another.
    fn mark(&mut self, from: Option<taffy::NodeId>, to: Option<taffy::NodeId>, class: &str) {
        if from == to {
            return;
        }
        if let Some(node) = from {
            self.set_class(NodeId(node), class, false);
        }
        if let Some(node) = to {
            self.set_class(NodeId(node), class, true);
        }
    }

    /// A label of what is dragged, in the layer above everything, that the
    /// pointer passes through.
    fn ghost(&mut self, source: DragSource) -> taffy::NodeId {
        let dragged = match source {
            DragSource::Row { list, index, .. } => self.row_node(list, index).unwrap_or(list.0),
            DragSource::Element(node) => node.0,
        };
        let text = self.first_text(dragged).unwrap_or_default();
        let layer = self.popup_layer_here();
        let mut style = Style { pickable: false, ..Style::row() };
        style.layout.position = Position::Absolute;
        let ghost = self.panel(layer, style);
        self.add_class(ghost, "drag-ghost");
        let label = Element {
            text: Some(Text::new(&text)),
            ..Element::new(Kind::Label, Style { pickable: false, ..Style::default() })
        };
        self.add(ghost, label);
        ghost.0
    }

    fn place_ghost(&mut self, point: (f32, f32)) {
        let Some(ghost) = self.drag.as_ref().and_then(|d| d.ghost).map(NodeId) else { return };
        let mut style = self.element(ghost).map(|e| e.base.clone()).unwrap_or_default();
        style.layout.inset.left = LengthPercentageAuto::length(point.0 + GHOST_OFFSET.0);
        style.layout.inset.top = LengthPercentageAuto::length(point.1 + GHOST_OFFSET.1);
        self.set_style(ghost, style);
    }

    /// The first text in an element or anything in it, such as a row's name.
    fn first_text(&self, node: taffy::NodeId) -> Option<String> {
        let own = self.element(NodeId(node)).and_then(|e| e.text.as_ref()).map(|_| self.text(NodeId(node)));
        if let Some(text) = own.filter(|t| !t.trim().is_empty()) {
            return Some(text);
        }
        self.tree.children(node).unwrap_or_default().into_iter().find_map(|child| self.first_text(child))
    }
}
