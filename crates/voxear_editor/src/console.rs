//! The Console: everything logged, newest at the bottom, following new
//! lines while it is scrolled to the end. Warnings and errors are coloured.
//! The plan for it follows CookieJarTools' Sifter (see docs/ARCHITECTURE.md).

use std::{cell::RefCell, rc::Rc};

use tracing::Level;
use voxear_core::logging::LogCapture;
use voxear_ui::{ItemFns, NodeId, Ui};

/// A line's height, as `item-height` gives it in Editor.vui.
const ROW: f32 = 20.0;

pub struct Console {
    list: NodeId,
    log: LogCapture,
    lines: Rc<RefCell<Vec<(Level, String)>>>,
    seen: u64,
}

impl Console {
    pub fn new(ui: &mut Ui, list: NodeId, log: LogCapture) -> Self {
        let lines: Rc<RefCell<Vec<(Level, String)>>> = Rc::default();
        let shown = lines.clone();
        let items = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                let line = ui.add_label(row, "");
                ui.add_class(line, "log-line");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let lines = shown.borrow();
                let Some((level, message)) = lines.get(index) else { return };
                let label = ui.children(row)[0];
                ui.set_text(label, &message.replace('\n', " "));
                ui.set_class(label, "log-warn", *level == Level::WARN);
                ui.set_class(label, "log-error", *level == Level::ERROR);
            },
        };
        ui.bind_list(list, 0, items);
        Self { list, log, lines, seen: 0 }
    }

    /// Shows lines logged since the last frame.
    pub fn update(&mut self, ui: &mut Ui) {
        let Ok(log) = self.log.lock() else { return };
        if log.total == self.seen {
            return;
        }
        self.seen = log.total;
        let before = self.lines.borrow().len();
        *self.lines.borrow_mut() = log.lines.iter().map(|l| (l.level, l.message.clone())).collect();
        drop(log);
        let count = self.lines.borrow().len();
        // Scrolled to the end, so new lines are followed; scrolled up, the view stays put.
        let [_, y] = ui.scroll_offset(self.list);
        let [_, _, _, height] = ui.viewport(self.list);
        let following = y + height >= before as f32 * ROW - ROW;
        ui.set_list_count(self.list, count);
        ui.refresh_list(self.list);
        if following && count > 0 {
            ui.scroll_to_index(self.list, count - 1);
        }
    }
}
