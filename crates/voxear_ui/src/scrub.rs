//! Dragging a number's label left or right changes the number, as Unity's
//! Inspector does: a label with the `scrub` class drives the first number
//! field after it in its row. The field's text changes as the pointer
//! moves, and it reads as changed, as typing in it would, so whatever reads
//! the field needs nothing more. Shift drags four times as fast.

use crate::{Kind, NodeId, Ui};

/// How much the number changes for each logical pixel dragged, for numbers
/// up to 1; larger numbers move faster, with the square root of their size,
/// so 100 moves ten times as fast as 1.
const PER_PIXEL: f64 = 0.03;

/// A scrub under way: the field, where the pointer went down, and the
/// number it started from.
pub(crate) struct Scrub {
    field: taffy::NodeId,
    from_x: f32,
    start: f64,
}

impl Ui {
    /// The number field a scrubbing label drives: the first enabled text
    /// field after it in its row, holding a number.
    fn scrub_field(&self, label: NodeId) -> Option<(NodeId, f64)> {
        if !self.has_class(label, "scrub") {
            return None;
        }
        let row = self.parent(label)?;
        let after = self.children(row).into_iter().skip_while(|&child| child != label).skip(1);
        let field = after.into_iter().find_map(|child| self.first_text_field(child))?;
        let number = self.text(field).trim().parse::<f64>().ok()?;
        self.is_enabled(field).then_some((field, number))
    }

    fn first_text_field(&self, node: NodeId) -> Option<NodeId> {
        if self.kind(node) == Some(Kind::TextField) {
            return Some(node);
        }
        self.children(node).into_iter().find_map(|child| self.first_text_field(child))
    }

    /// Whether the pointer is over a label that would scrub, or scrubbing:
    /// the window shows a left-right cursor then.
    pub fn wants_scrub_cursor(&self) -> bool {
        self.scrub.is_some() || self.pointer_hovered().is_some_and(|node| self.scrub_field(node).is_some())
    }

    pub fn is_scrubbing(&self) -> bool {
        self.scrub.is_some()
    }

    /// A press on a scrubbing label starts a scrub; whether it did.
    pub(crate) fn press_scrub(&mut self, path: &[taffy::NodeId], point: (f32, f32)) -> bool {
        let Some(&label) = path.last() else { return false };
        let Some((field, start)) = self.scrub_field(NodeId(label)) else { return false };
        // Whatever was being typed is done with first.
        self.blur();
        self.scrub = Some(Scrub { field: field.0, from_x: point.0, start });
        true
    }

    /// The pointer moved while scrubbing: the field takes the number so far.
    pub(crate) fn drag_scrub(&mut self, point: (f32, f32)) {
        let Some(scrub) = &self.scrub else { return };
        let field = NodeId(scrub.field);
        let speed = PER_PIXEL * scrub.start.abs().sqrt().max(1.0) * if self.modifiers.shift { 4.0 } else { 1.0 };
        let value = scrub.start + f64::from(point.0 - scrub.from_x) * speed;
        let text = format!("{value:.3}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        let text = if text == "-0" { "0" } else { text };
        if self.text(field) != text {
            self.set_text(field, text);
            if !self.input.changed.contains(&field.0) {
                self.input.changed.push(field.0);
            }
        }
    }

    /// Letting go ends a scrub; whether one was under way.
    pub(crate) fn release_scrub(&mut self) -> bool {
        self.scrub.take().is_some()
    }

    /// A field taken out mid-scrub ends it.
    pub(crate) fn forget_scrub(&mut self, node: taffy::NodeId) {
        if self.scrub.as_ref().is_some_and(|scrub| scrub.field == node) {
            self.scrub = None;
        }
    }
}
