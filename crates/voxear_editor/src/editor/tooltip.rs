//! Tooltips: what a script field's `[Tooltip]` says, shown under its label
//! once the pointer has rested there for half a second, and gone as soon as
//! it moves off.

use super::Editor;

/// How long the pointer rests on a label before its tooltip shows.
const DELAY: f32 = 0.5;

impl Editor {
    pub(super) fn update_tooltip(&mut self, delta: f32) {
        let tooltip = self.nodes.tooltip;
        let Some((text, label)) = self.inspector.hovered_tooltip(&self.ui) else {
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
        let [x, y, _, height] = self.ui.rect(label);
        self.ui.set_inline_style(tooltip, &format!("left: {x}px; top: {}px", y + height + 4.0));
        self.ui.set_visible(tooltip, true);
    }
}
