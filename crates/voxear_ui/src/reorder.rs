//! Reordering: the items of a container dragged by a handle into a new
//! order, as in Unity's reorderable lists. A press on a handle that moves a
//! few pixels lifts its item, which follows the pointer up and down; the
//! other items slide out of its way; and once let go, everything eases into
//! place. [`Ui::reordered`] reports the move for the caller to make in its
//! data. The items keep their elements and show the moved data, so each is
//! eased from where it was drawn to where it now goes.
//!
//! The slides move what is drawn, not the layout, as a CSS transform does,
//! so nothing is laid out again while they play.

use std::collections::{HashMap, HashSet};

use crate::{NodeId, Ui};

/// How far a press must move to lift its item rather than be a click.
const LIFT: f32 = 4.0;
/// How quickly items ease to where they are going: the share of the way
/// left covered each second, as an exponential rate. About a fifth of a
/// second to settle.
const RATE: f32 = 22.0;
/// Close enough to stop easing, in logical pixels.
const SETTLED: f32 = 0.3;

#[derive(Default)]
pub(crate) struct Reorders {
    containers: HashSet<taffy::NodeId>,
    handles: HashSet<taffy::NodeId>,
    /// Each item's offset down from where layout puts it, now and to come.
    slides: HashMap<taffy::NodeId, Slide>,
    lift: Option<Lift>,
    /// Moves let go of since the last frame: the container, from and to.
    done: Vec<(taffy::NodeId, usize, usize)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Slide {
    now: f32,
    to: f32,
}

/// An item pressed by its handle, and once it moves, lifted.
struct Lift {
    container: taffy::NodeId,
    items: Vec<taffy::NodeId>,
    from: usize,
    /// Where the pointer was pressed, down the screen.
    press: f32,
    moving: bool,
    /// Each item's top and height as laid out when it was pressed, and the
    /// space between items.
    tops: Vec<f32>,
    heights: Vec<f32>,
    gap: f32,
    /// Where it would land if let go now.
    to: usize,
}

impl Reorders {
    pub(crate) fn forget(&mut self, node: taffy::NodeId) {
        self.containers.remove(&node);
        self.handles.remove(&node);
        self.slides.remove(&node);
        if self.lift.as_ref().is_some_and(|lift| lift.container == node || lift.items.contains(&node)) {
            self.lift = None;
        }
    }

    pub(crate) fn frame_done(&mut self) {
        self.done.clear();
    }
}

impl Ui {
    /// Lets the children of `container` be dragged into a new order by
    /// their handles (see [`set_reorder_handle`](Self::set_reorder_handle)).
    pub fn set_reorderable(&mut self, container: NodeId, reorderable: bool) {
        if reorderable {
            self.reorders.containers.insert(container.0);
        } else {
            self.reorders.containers.remove(&container.0);
        }
    }

    /// Makes `handle`, inside an item of a reorderable container, what drags
    /// the item. An icon, which the pointer passes through, takes presses
    /// once it is a handle.
    pub fn set_reorder_handle(&mut self, handle: NodeId, on: bool) {
        if on {
            self.reorders.handles.insert(handle.0);
        } else {
            self.reorders.handles.remove(&handle.0);
        }
        if let Some(element) = self.element_mut(handle) {
            element.base.pickable |= on;
            self.restyle(handle.0, false);
        }
    }

    /// The move let go of in `container` since the last frame: the item's
    /// place before, and after.
    pub fn reordered(&self, container: NodeId) -> Option<(usize, usize)> {
        let found = self.reorders.done.iter().find(|(at, ..)| *at == container.0);
        found.map(|&(_, from, to)| (from, to))
    }

    /// The container whose item is being dragged, once it has lifted.
    pub fn reordering(&self) -> Option<NodeId> {
        self.reorders.lift.as_ref().filter(|lift| lift.moving).map(|lift| NodeId(lift.container))
    }

    /// How far an item is drawn from where it is laid out, down the screen,
    /// as it slides.
    pub fn slide(&self, node: NodeId) -> f32 {
        self.reorders.slides.get(&node.0).map_or(0.0, |slide| slide.now)
    }

    /// The slides of a container's items, top to bottom, to carry over to
    /// the items of a container built again in its place, as a view is
    /// after the change a move makes.
    pub fn slides_of(&self, container: NodeId) -> Vec<f32> {
        let items = self.tree.children(container.0).unwrap_or_default();
        items.into_iter().map(|item| self.slide(NodeId(item))).collect()
    }

    /// Starts the items of `container`, top to bottom, slid by `slides`,
    /// easing to their places.
    pub fn start_slides(&mut self, container: NodeId, slides: &[f32]) {
        let items = self.tree.children(container.0).unwrap_or_default();
        for (item, &now) in items.into_iter().zip(slides).filter(|(_, now)| **now != 0.0) {
            self.reorders.slides.insert(item, Slide { now, to: 0.0 });
            if let Some(element) = self.tree.get_node_context_mut(item) {
                element.translate[1] = now;
            }
        }
    }

    /// A press, which picks up an item when it is on a handle in one: the
    /// item nearest the handle whose container reorders.
    pub(crate) fn press_reorder(&mut self, path: &[taffy::NodeId], point: (f32, f32)) -> bool {
        let Some(handle_at) = path.iter().rposition(|node| self.reorders.handles.contains(node)) else {
            return false;
        };
        let Some(container_at) = path[..handle_at].iter().rposition(|node| self.reorders.containers.contains(node))
        else {
            return false;
        };
        let (container, item) = (path[container_at], path[container_at + 1]);
        let items: Vec<taffy::NodeId> = self
            .tree
            .children(container)
            .unwrap_or_default()
            .into_iter()
            .filter(|&child| self.tree.style(child).is_ok_and(|s| s.display != taffy::Display::None))
            .collect();
        let Some(from) = items.iter().position(|&i| i == item) else { return false };
        let laid = |node: taffy::NodeId| self.tree.layout(node).map_or((0.0, 0.0), |l| (l.location.y, l.size.height));
        let (tops, heights): (Vec<f32>, Vec<f32>) = items.iter().map(|&i| laid(i)).unzip();
        let gap = if items.len() > 1 { (tops[1] - tops[0] - heights[0]).max(0.0) } else { 0.0 };
        self.reorders.lift =
            Some(Lift { container, items, from, press: point.1, moving: false, tops, heights, gap, to: from });
        true
    }

    /// The pointer moved with an item pressed: past a few pixels it lifts
    /// and follows, and the others make room where it would land. True
    /// while an item is pressed, as nothing else drags then.
    pub(crate) fn move_reorder(&mut self, point: (f32, f32)) -> bool {
        let Some(lift) = &mut self.reorders.lift else { return false };
        let down = point.1 - lift.press;
        if !lift.moving {
            if down.abs() < LIFT {
                return true;
            }
            lift.moving = true;
            let (container, item) = (lift.container, lift.items[lift.from]);
            self.pointer_cancel();
            self.set_class(NodeId(container), "reordering", true);
            self.set_class(NodeId(item), "reorder-lifted", true);
        }
        let Some(lift) = &mut self.reorders.lift else { return false };
        let last = lift.items.len() - 1;
        let (top, height) = (lift.tops[lift.from], lift.heights[lift.from]);
        let down = down.clamp(-top + lift.tops[0], lift.tops[last] + lift.heights[last] - top - height);
        // Going up, it takes the place of the highest item whose middle its top has passed; going down,
        // of the lowest whose middle its bottom has passed. Its edges, not its middle, so an item a little
        // taller than the first can still go first, held at the list's top.
        let middle_of = |i: usize| lift.tops[i] + lift.heights[i] / 2.0;
        let (upper, lower) = (top + down, top + down + height);
        lift.to = match down < 0.0 {
            true => (0..lift.from).find(|&i| upper < middle_of(i)).unwrap_or(lift.from),
            false => (lift.from + 1..lift.items.len()).rev().find(|&i| lower > middle_of(i)).unwrap_or(lift.from),
        };
        let room = height + lift.gap;
        let (from, to) = (lift.from, lift.to);
        for (index, &item) in lift.items.iter().enumerate() {
            let slide = self.reorders.slides.entry(item).or_default();
            if index == from {
                *slide = Slide { now: down, to: down };
            } else if from < index && index <= to {
                slide.to = -room;
            } else if to <= index && index < from {
                slide.to = room;
            } else {
                slide.to = 0.0;
            }
        }
        true
    }

    /// Whether an item is pressed, lifted or about to be.
    pub(crate) fn reorder_pressed(&self) -> bool {
        self.reorders.lift.is_some()
    }

    /// Lets go of a pressed item: once it lifted, the move is reported and
    /// every item eases from where it is drawn to its place in the new
    /// order. True when it had lifted, as the release is then used up.
    pub(crate) fn release_reorder(&mut self) -> bool {
        let Some(lift) = self.reorders.lift.take() else { return false };
        if !lift.moving {
            return false;
        }
        self.set_class(NodeId(lift.container), "reordering", false);
        self.set_class(NodeId(lift.items[lift.from]), "reorder-lifted", false);
        let (from, to) = (lift.from, lift.to);
        let drawn: Vec<f32> = (0..lift.items.len()).map(|i| lift.tops[i] + self.slide(NodeId(lift.items[i]))).collect();
        if from == to {
            for item in &lift.items {
                self.reorders.slides.entry(*item).or_default().to = 0.0;
            }
            return true;
        }
        self.reorders.done.push((lift.container, from, to));
        // Once the caller moves its data, each item shows what the item at
        // `order[k]` did, its height going with it.
        let mut order: Vec<usize> = (0..lift.items.len()).collect();
        let moved = order.remove(from);
        order.insert(to, moved);
        let mut top = lift.tops[0];
        for (k, &shown) in order.iter().enumerate() {
            let now = drawn[shown] - top;
            self.reorders.slides.insert(lift.items[k], Slide { now, to: 0.0 });
            top += lift.heights[shown] + lift.gap;
        }
        true
    }

    /// Eases the slides on by a frame's time, and draws each item where it
    /// has got to.
    pub(crate) fn step_reorders(&mut self, seconds: f32) {
        let ease = 1.0 - (-RATE * seconds.max(0.0)).exp();
        let lifted = self.reorders.lift.as_ref().filter(|l| l.moving).map(|l| l.items[l.from]);
        for (node, slide) in &mut self.reorders.slides {
            if Some(*node) != lifted {
                slide.now += (slide.to - slide.now) * ease;
                if (slide.to - slide.now).abs() < SETTLED {
                    slide.now = slide.to;
                }
            }
        }
        let slides: Vec<(taffy::NodeId, Slide)> = self.reorders.slides.iter().map(|(n, s)| (*n, *s)).collect();
        for (node, slide) in slides {
            if let Some(element) = self.tree.get_node_context_mut(node) {
                element.translate[1] = slide.now;
            }
        }
        self.reorders.slides.retain(|_, slide| slide.now != 0.0 || slide.to != 0.0);
    }
}
