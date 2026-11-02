//! Tooltips: what a script field's `[Tooltip]` says, shown under its label,
//! and what one of the game's side bar buttons does, shown beside it, once
//! the pointer has rested there for half a second, and gone as soon as it
//! moves off.

use super::Editor;

/// How long the pointer rests on a label before its tooltip shows.
const DELAY: f32 = 0.5;

impl Editor {
    pub(super) fn update_tooltip(&mut self, delta: f32) {
        let tooltip = self.nodes.tooltip;
        let beside = self.side_bar_tooltip();
        let Some((text, label)) = self.inspector.hovered_tooltip(&self.ui).or(beside.clone()) else {
            self.hovered_tooltip = None;
            self.ui.set_visible(tooltip, false);
            return;
        };
        let rested = match self.hovered_tooltip.take() {
            Some((shown, time)) if shown == text => time + delta,
            _ => 0.0,
        };
        self.hovered_tooltip = Some((text.clone(), rested));
        if rested < DELAY {
            return;
        }
        if self.ui.text(tooltip) != text {
            self.ui.set_text(tooltip, &text);
        }
        self.ui.show_over_window(tooltip, self.ui.window_of(label));
        let [x, y, width, height] = self.ui.rect(label);
        let (left, top) = if beside.is_some_and(|(_, button)| button == label) {
            (x + width + 6.0, y + 6.0)
        } else {
            (x, y + height + 4.0)
        };
        self.ui.set_inline_style(tooltip, &format!("left: {left}px; top: {top}px"));
        self.ui.set_visible(tooltip, true);
    }
}
